import {
  TYPES,
  normalizeOcrText,
  selectClotheslinePhotos,
  cropRect,
  naturalCrop,
  getLinks,
  matchesNote,
  toMarkdown,
  validateCollection,
} from "./core.js";

const $ = (s) => document.querySelector(s),
  $$ = (s) => [...document.querySelectorAll(s)];
const native = window.__TAURI__;
const invoke = (command, args = {}) =>
  native
    ? native.core.invoke(command, args)
    : Promise.reject(Error("请在桌面版中使用此功能"));
const state = { version: 1, photos: [], notes: [] };
let desktopOptions = {autoMount:true,photoLimit:6};
let activeId = null,
  view = "workbench",
  query = "",
  filter = "all",
  selection = null,
  selectionOrigin = null,
  selectionDisplay = null,
  editId = null,
  notePhotoId = null,
  noteType = "text",
  ocrToken = 0,
  saveTimer,
  toastTimer,
  storageHealthy = true,
  saveQueue = Promise.resolve(),
  receiving = Promise.resolve();
const uid = () => crypto.randomUUID();
const current = () => state.photos.find((p) => p.id === activeId);
const icon = (name) => {
  const s = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  const u = document.createElementNS(s.namespaceURI, "use");
  u.setAttribute("href", "#i-" + name);
  s.append(u);
  s.setAttribute("aria-hidden", "true");
  return s;
};
function element(tag, cls, text) {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
}
function iconButton(name, label, action) {
  const b = element("button", "icon-button");
  b.type = "button";
  b.title = label;
  b.setAttribute("aria-label", label);
  b.append(icon(name));
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    action();
  });
  return b;
}
function toast(message, error = false, action) {
  clearTimeout(toastTimer);
  const e = $("#toast");
  e.className = "toast" + (error ? " error" : "");
  e.replaceChildren(document.createTextNode(message));
  if (action) {
    const b = element("button", "", action.label);
    b.addEventListener("click", () => {
      action.run();
      e.hidden = true;
    });
    e.append(b);
  }
  e.hidden = false;
  toastTimer = setTimeout(() => (e.hidden = true), error ? 8500 : 4200);
}
function persist(immediate = false) {
  if (!native) {
    $("#save-status").textContent = "界面预览 · 桌面版会自动保存";
    return;
  }
  if (!storageHealthy) {
    toast("原有收藏读取失败，已停止自动保存，以保留原文件。", true);
    if (immediate) return Promise.reject(Error("原有收藏读取失败，已保留原文件"));
    return;
  }
  $("#save-status").textContent = "正在收好…";
  clearTimeout(saveTimer);
  const save = () => {
    const snapshot = JSON.parse(JSON.stringify(state));
    saveQueue = saveQueue
      .catch(() => {})
      .then(async () => {
        try {
          await invoke("save_state", { data: snapshot });
          $("#save-status").textContent = "已收好 · 所有内容留在本机";
        } catch (e) {
          $("#save-status").textContent = "保存失败 · 请导出备份";
          toast("保存失败：" + String(e), true);
          throw e;
        }
      });
    return saveQueue;
  };
  if (immediate) return save();
  saveTimer = setTimeout(() => save().catch(() => {}), 180);
}
function setView(next) {
  view = next;
  $$(".tab").forEach((b) =>
    b.classList.toggle("active", b.dataset.view === next),
  );
  $$(".view").forEach((v) =>
    v.classList.toggle("active", v.id === next + "-view"),
  );
  renderCollections();
}
function timeLabel(ts) {
  return new Date(ts).toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
  });
}
function dateLabel(ts) {
  const date = new Date(ts);
  return (
    date.toLocaleDateString("zh-CN", { month: "2-digit", day: "2-digit" }) +
    " " +
    timeLabel(ts)
  );
}
function clearSelection() {
  selection = null;
  selectionOrigin = null;
  selectionDisplay = null;
  $("#selection").hidden = true;
  $("#clear-selection").hidden = true;
  $("#selection-hint").textContent = current()
    ? "拖动框选文字 · 不框选则识别整张"
    : "照片留在本机，灵感自在生长";
}
function setActive(id) {
  activeId = id;
  clearSelection();
  const photo = current();
  updatePinButton();
  $("#empty-stage").hidden = !!photo;
  $("#image-stage").hidden = !photo;
  $("#recognize-button").disabled = !photo;
  $("#archive-button").disabled = !photo;
  $("#photo-label").textContent = photo
    ? photo.demo
      ? "试洗样片"
      : timeLabel(photo.createdAt) +
        " · " +
        (photo.archived ? "已经晾好" : "等待显影")
    : "等一张新照片";
  if (photo) {
    $("#source-image").src = photo.dataUrl;
    $("#dropzone").classList.remove("washing");
    requestAnimationFrame(() => $("#dropzone").classList.add("washing"));
    setTimeout(() => $("#dropzone").classList.remove("washing"), 850);
    $("#archive-button").replaceChildren(
      icon("check"),
      document.createTextNode(photo.archived ? "已晾好" : "洗好了"),
    );
  }
  renderLine();
}
async function normalizeImage(dataUrl) {
  const image = new Image();
  image.src = dataUrl;
  await image.decode();
  if (image.naturalWidth * image.naturalHeight > 60_000_000)
    throw Error("图片过大，请分区域截取");
  const scale = Math.min(
    1,
    4000 / Math.max(image.naturalWidth, image.naturalHeight),
  );
  if (scale === 1 && dataUrl.startsWith("data:image/png")) return dataUrl;
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(image.naturalWidth * scale));
  canvas.height = Math.max(1, Math.round(image.naturalHeight * scale));
  const c = canvas.getContext("2d");
  c.fillStyle = "#fff";
  c.fillRect(0, 0, canvas.width, canvas.height);
  c.drawImage(image, 0, 0, canvas.width, canvas.height);
  return canvas.toDataURL("image/png");
}
async function addPhoto(dataUrl, name, options = {}) {
  if (state.photos.length >= 300)
    throw Error("照片较多，请先备份并移走部分照片");
  if (typeof dataUrl !== "string" || dataUrl.length > 65_000_000)
    throw Error("图片太大，请选择小于 40 MB 的图片");
  const normalized = await normalizeImage(dataUrl);
  const previous = state.photos[0];
  if (options.dedupe && previous?.dataUrl === normalized) {
    if (!$("#note-dialog").open) {
      setActive(previous.id);
      setView("workbench");
    }
    return previous;
  }
  const photo = {
    id: uid(),
    name: name.slice(0, 200),
    dataUrl: normalized,
    createdAt: Date.now(),
    archived: false,
    demo: !!options.demo,
  };
  state.photos.unshift(photo);
  if (!$("#note-dialog").open) {
    setActive(photo.id);
    setView("workbench");
  }
  dismissTip();
  persist();
  renderCounts();
  renderLine();
  renderCollections();
  return photo;
}
function readFile(file) {
  return new Promise((resolve, reject) => {
    const r = new FileReader();
    r.onload = () => resolve(r.result);
    r.onerror = () => reject(Error("图片读取失败"));
    r.readAsDataURL(file);
  });
}
async function importImages(files) {
  let count = 0;
  for (const file of files) {
    try {
      if (!file.type.startsWith("image/")) {
        toast("请放入图片文件。", true);
        continue;
      }
      if (file.size > 40 * 1024 * 1024) throw Error("请选择小于 40 MB 的图片");
      await addPhoto(await readFile(file), file.name);
      count++;
    } catch (e) {
      toast(String(e), true);
    }
  }
  if (count)
    toast(
      count === 1
        ? "新照片入水了。拖动框选，让文字显影。"
        : `接住了 ${count} 张照片。`,
    );
}
async function receiveCaptures() {
  if (!native) return;
  const photos = await invoke("take_captures");
  let accepted=0;
  for (const url of photos) {
    await addPhoto(url, "截图 · " + new Date().toLocaleString("zh-CN"), {
      dedupe: true,
    });
    accepted++;
  }
  if (accepted) {
    await persist(true);
    if (desktopOptions.autoMount) updateDesktopLine(await invoke("set_desktop_line", {enabled:true}));
    toast(
      $("#note-dialog").open
        ? "新截图已经夹好，写完这张便签再来看看。"
        : desktopOptions.autoMount ? "新截图已挂到桌面晾片绳。点击照片再整理。" : "新截图已安静收好。",
    );
  }
}
function queueCapture() {
  receiving = receiving
    .catch(() => {})
    .then(receiveCaptures)
    .catch((e) => toast("截图接收失败：" + String(e), true));
}
function updateMonitor(enabled) {
  $("#monitor-toggle").setAttribute("aria-pressed", String(enabled));
  $("#monitor-label").textContent = enabled ? "自动接收中" : "自动接收已暂停";
}
function updateDesktopLine(preferences) {
  desktopOptions={...desktopOptions,...preferences};
  const enabled = !!preferences.enabled;
  $("#desktop-line-toggle").setAttribute("aria-pressed", String(enabled));
  $("#desktop-line-toggle").textContent = enabled ? "收回冲洗室 ↙" : "挂到桌面 ↗";
  $("#desktop-line-setting").checked = enabled;
  $("#clothesline").hidden = enabled;
  $("#desktop-line-hint").hidden = !enabled;
  $("#auto-mount-setting").checked=desktopOptions.autoMount;
  $("#photo-limit-setting").value=String(desktopOptions.photoLimit);
}
async function changeDesktopOptions() {
  try {updateDesktopLine(await invoke("set_desktop_options",{autoMount:$("#auto-mount-setting").checked,photoLimit:Number($("#photo-limit-setting").value)}));}
  catch(error){updateDesktopLine(desktopOptions);toast(String(error),true);}
}
$("#auto-mount-setting").addEventListener("change",changeDesktopOptions);
$("#photo-limit-setting").addEventListener("change",changeDesktopOptions);
function updatePinButton() {
  const photo=current();const button=$("#pin-current");
  button.disabled=!photo;button.textContent=photo?.pinned?"★ 已固定":"☆ 固定";
  button.setAttribute("aria-pressed",String(!!photo?.pinned));
}
function togglePhotoPin(id) {
  const photo=state.photos.find(item=>item.id===id);if(!photo)return;
  photo.pinned=!photo.pinned;persist();updatePinButton();renderLine();renderCollections();
  toast(photo.pinned?"这张照片会优先留在晾片绳上。":"已取消固定。");
}
$("#pin-current").addEventListener("click",()=>{if(current())togglePhotoPin(current().id);});
async function changeDesktopLine(enabled) {
  $("#desktop-line-toggle").disabled = true;
  $("#desktop-line-setting").disabled = true;
  try {
    updateDesktopLine(await invoke("set_desktop_line", {enabled}));
    toast(enabled ? "晾片绳挂到桌面了。收起冲洗室，就能看到。" : "晾片绳已经收回冲洗室。");
  } catch (error) {
    $("#desktop-line-setting").checked = $("#desktop-line-toggle").getAttribute("aria-pressed") === "true";
    toast(String(error), true);
  } finally {
    $("#desktop-line-toggle").disabled = false;
    $("#desktop-line-setting").disabled = false;
  }
}
$("#desktop-line-toggle").addEventListener("click", () => changeDesktopLine($("#desktop-line-toggle").getAttribute("aria-pressed") !== "true"));
$("#desktop-line-setting").addEventListener("change", event => changeDesktopLine(event.target.checked));

function buildNote(note, full = false) {
  const meta = TYPES[note.type],
    card = element(
      "article",
      "note-card type-" + note.type + (note.done ? " done" : ""),
    );
  card.dataset.noteId = note.id;
  const header = element("div", "note-card-meta"),
    label = element("span");
  label.append(icon(meta.icon), document.createTextNode(meta.label));
  header.append(label, element("span", "", dateLabel(note.createdAt)));
  card.append(header, element("p", "note-content", note.text));
  if (note.tags.length) {
    const tags = element("div", "note-tags");
    note.tags.forEach((t) => tags.append(element("span", "note-tag", "#" + t)));
    card.append(tags);
  }
  const actions = element("div", "note-actions");
  if (note.photoId && state.photos.some((p) => p.id === note.photoId)) {
    const source = element("button", "source-link", "回到原图 ↗");
    source.addEventListener("click", () => {
      setActive(note.photoId);
      setView("workbench");
    });
    actions.append(source);
  }
  if (note.type === "link" && getLinks(note.text).length)
    actions.append(iconButton("link", "在浏览器打开链接", () =>
      invoke("open_url", {url:getLinks(note.text)[0]}).catch(e=>toast(String(e),true))
    ));
  if (note.type === "todo")
    actions.append(
      iconButton("check", note.done ? "标为未完成" : "标为已完成", () => {
        note.done = !note.done;
        persist();
        renderNotes();
        renderCollections();
      }),
    );
  actions.append(
    iconButton("copy", "复制便签", () => copyText(note.text)),
    iconButton("edit", "编辑便签", () => openNote(note)),
    iconButton("trash", "移走便签（可撤销）", () => removeNote(note.id)),
  );
  card.append(actions);
  return card;
}
function emptyNotes(text = "圈出截图里的文字，\n或手写一张小便签。") {
  const e = element("div", "note-empty");
  const art = element("div", "empty-notes-art");
  const a = element("span"),
    b = element("span");
  a.append(icon("note"));
  b.append(icon("spark"));
  art.append(a, b);
  e.append(art);
  const p = element("p");
  p.style.whiteSpace = "pre-line";
  p.textContent = text;
  e.append(p);
  return e;
}
function renderNotes() {
  const notes = state.notes.filter((n) => matchesNote(n, query, filter));
  $("#note-list").replaceChildren(
    ...(notes.length
      ? notes.map((n) => buildNote(n))
      : [
          emptyNotes(
            query || filter !== "all"
              ? "还没有找到匹配的便签。\n试试其他关键词或类型。"
              : undefined,
          ),
        ]),
  );
  $("#note-summary").textContent = state.notes.length
    ? `${state.notes.length} 张便签，${state.notes.filter((n) => n.type === "todo" && !n.done).length} 件小事待完成`
    : "留点空白，等灵感来。";
  renderCounts();
}
function renderCounts() {
  $("#notes-total").textContent = state.notes.length;
  $("#photos-total").textContent = state.photos.length;
  $("#drying-count").textContent = state.photos.length;
}
function photoButton(photo, index, album = false) {
  const b = element("button", "hanging-photo");
  b.style.setProperty("--tilt", [-4, 3, -2, 4, -3, 2][index % 6] + "deg");
  const image = element("img");
  image.src = photo.dataUrl;
  image.alt = photo.name;
  image.loading = "lazy";
  b.append(
    image,
    element("span", "photo-caption", photo.demo ? "试洗样片" : photo.name),
  );
  if (photo.archived) {
    const badge = element("span", "washed-badge");
    badge.append(icon("check"));
    b.append(badge);
  }
  if(photo.pinned)b.append(element("span","pinned-badge","★"));
  b.title = "打开：" + photo.name;
  b.addEventListener("click", () => {
    setActive(photo.id);
    setView("workbench");
  });
  if (photo.id === activeId) b.style.outline = "1px solid #9aaa7c";
  return b;
}
function renderLine() {
  const line = $("#clothesline");
  line.replaceChildren(
    ...selectClotheslinePhotos(state.photos,7).map((p, i) => photoButton(p, i)),
  );
  if (!state.photos.length) {
    for (let i = 0; i < 3; i++) {
      const placeholder = element("div", "hanging-photo hanging-placeholder");
      placeholder.style.setProperty("--tilt", [-4, 3, -2][i] + "deg");
      const inside = element("div", "placeholder-inside");
      inside.append(icon(["photo", "note", "spark"][i]));
      placeholder.append(
        inside,
        element(
          "span",
          "photo-caption",
          ["第一张新照片", "一句有用的话", "一点小小灵感"][i],
        ),
      );
      line.append(placeholder);
    }
    line.append(
      element("span", "line-caption", "good things take a little time."),
    );
  }
}
function renderCollections() {
  if (view === "notes") {
    const term = $("#collection-search").value;
    const notes = state.notes.filter((n) => matchesNote(n, term));
    $("#all-notes").replaceChildren(
      ...(notes.length
        ? notes.map((n) => buildNote(n, true))
        : [
            emptyNotes(
              term
                ? "没有找到这张便签。"
                : "便签册还是空白的。\n去冲洗台，留住第一点灵感吧。",
            ),
          ]),
    );
  }
  if (view === "album") {
    const cards = state.photos.map((p, i) => {
      const div = element("div", "album-card");
      div.append(photoButton(p, i, true));
      const actions = element("div", "album-card-actions");
      actions.append(
        element("span", "", dateLabel(p.createdAt)),
        iconButton("spark",p.pinned?"取消固定到晾片绳":"固定到晾片绳",()=>togglePhotoPin(p.id)),
        iconButton("trash", "从收藏移走照片（可撤销）", () =>
          removePhoto(p.id),
        ),
      );
      div.append(actions);
      return div;
    });
    $("#album-grid").replaceChildren(
      ...(cards.length
        ? cards
        : [
            emptyNotes(
              "晾片夹空空的。\n拖入一张图片，或按 Win + Shift + S 截图。",
            ),
          ]),
    );
  }
}
function removeNote(id) {
  const i = state.notes.findIndex((n) => n.id === id);
  if (i < 0) return;
  const [note] = state.notes.splice(i, 1);
  persist();
  renderNotes();
  renderCollections();
  toast("便签已移走。", false, {
    label: "撤销",
    run: () => {
      if (!state.notes.some((n) => n.id === id)) state.notes.splice(i, 0, note);
      persist();
      renderNotes();
      renderCollections();
    },
  });
}
function removePhoto(id) {
  const i = state.photos.findIndex((p) => p.id === id);
  if (i < 0) return;
  const [photo] = state.photos.splice(i, 1);
  if (activeId === id) setActive(state.photos[0]?.id || null);
  persist();
  renderCounts();
  renderLine();
  renderCollections();
  toast("照片已从收藏移走，提取的便签仍保留。", false, {
    label: "撤销",
    run: () => {
      if (!state.photos.some((p) => p.id === id))
        state.photos.splice(i, 0, photo);
      persist();
      renderCounts();
      renderLine();
      renderCollections();
    },
  });
}
async function copyText(text) {
  try {
    if (native) await invoke("copy_text", { text });
    else await navigator.clipboard.writeText(text);
    toast("已复制，可以带到别的地方了。");
  } catch (e) {
    toast("复制失败：" + String(e), true);
  }
}
function setNoteType(type) {
  noteType = type;
  $$(".note-type").forEach((b) =>
    b.classList.toggle("active", b.dataset.type === type),
  );
}
function openNote(note = null, sourceId = activeId) {
  editId = note?.id || null;
  notePhotoId = note?.photoId ?? sourceId;
  ocrToken++;
  setNoteType(note?.type || "text");
  $("#dialog-title").textContent = note ? "把便签再理一理" : "手写一张小便签";
  $("#note-text").value = note?.text || "";
  $("#note-tags").value = note?.tags?.join(" ") || "";
  $("#crop-preview").hidden = true;
  $("#ocr-status").hidden = true;
  $("#save-note").disabled = false;
  $("#save-note").replaceChildren(
    icon("check"),
    document.createTextNode(note ? "收好修改" : "夹进便签册"),
  );
  $("#dialog-source").textContent =
    state.photos.find((p) => p.id === notePhotoId)?.name || "保存在本机便签册";
  if (!$("#note-dialog").open) $("#note-dialog").showModal();
  setTimeout(() => $("#note-text").focus(), 50);
}
function closeNote() {
  ocrToken++;
  $("#note-dialog").close();
  $("#dropzone").classList.remove("washing");
}
function cropCurrent() {
  const photo = current(),
    image = $("#source-image");
  if (!photo || !image.complete || !image.naturalWidth)
    throw Error("图片还没准备好，请稍后重试");
  const bounds = $("#image-wrap").getBoundingClientRect();
  const crop = naturalCrop(
    selection,
    bounds.width,
    bounds.height,
    image.naturalWidth,
    image.naturalHeight,
  );
  const scale = Math.min(2, 4000 / Math.max(crop.w, crop.h));
  const canvas = document.createElement("canvas");
  canvas.width = Math.max(1, Math.round(crop.w * scale));
  canvas.height = Math.max(1, Math.round(crop.h * scale));
  const c = canvas.getContext("2d");
  c.fillStyle = "#fff";
  c.fillRect(0, 0, canvas.width, canvas.height);
  c.drawImage(
    image,
    crop.x,
    crop.y,
    crop.w,
    crop.h,
    0,
    0,
    canvas.width,
    canvas.height,
  );
  return canvas.toDataURL("image/png");
}
async function recognize() {
  try {
    const dataUrl = cropCurrent(),
      sourceId = activeId;
    openNote(null, sourceId);
    const token = ++ocrToken;
    $("#dialog-title").textContent = "从截图里，显影一点灵感";
    $("#crop-image").src = dataUrl;
    $("#crop-preview").hidden = false;
    $("#ocr-status").hidden = false;
    $("#ocr-status").className = "ocr-status";
    $("#ocr-status .spinner").style.display = "";
    $("#ocr-status").lastElementChild.textContent = "正在本机显影文字…";
    $("#save-note").disabled = true;
    const result = await invoke("recognize_image", {
      dataUrl,
      language: localStorage.getItem("darkroom-language") || "auto",
    });
    if (token !== ocrToken) return;
    $("#save-note").disabled = false;
    $("#note-text").value = normalizeOcrText(result.text);
    $("#ocr-status").lastElementChild.textContent = result.text
      ? `显影完成 · ${result.language} · 文字可以自由修改`
      : "这里没找到清晰的文字。可以重新框选，或手动写一点。";
    $("#ocr-status").classList.toggle("error", !result.text);
    $("#ocr-status .spinner").style.display = "none";
    if (getLinks(result.text).length) setNoteType("link");
    $("#note-text").focus();
  } catch (e) {
    $("#save-note").disabled = false;
    $("#ocr-status").hidden = false;
    $("#ocr-status").className = "ocr-status error";
    $("#ocr-status").lastElementChild.textContent = String(e);
  }
}
function saveNote(event) {
  event.preventDefault();
  const text = $("#note-text").value.trim();
  if (!text) {
    $("#note-text").focus();
    return;
  }
  const tags = [
    ...new Set(
      $("#note-tags")
        .value.split(/[\s,，]+/)
        .map((t) => t.replace(/^#+/, ""))
        .filter(Boolean),
    ),
  ].slice(0, 12);
  if (noteType === "link" && !getLinks(text).length) {
    toast("没有找到完整的 http / https 链接。可以改为摘录保存。", true);
    return;
  }
  if (editId) {
    const note = state.notes.find((n) => n.id === editId);
    if (note) Object.assign(note, { text, type: noteType, tags });
  } else {
    state.notes.unshift({
      id: uid(),
      text,
      type: noteType,
      tags,
      photoId: notePhotoId,
      createdAt: Date.now(),
      done: false,
    });
  }
  closeNote();
  persist();
  renderNotes();
  renderCollections();
  toast("夹好了，有用的那一点留下了。");
}
function archive() {
  const photo = current();
  if (!photo) return;
  if (photo.archived) {
    toast("这张照片已经晾好了。可以继续提取文字。");
    return;
  }
  photo.archived = true;
  $("#image-stage").classList.add("archive-animation");
  setTimeout(() => {
    $("#image-stage").classList.remove("archive-animation");
    const next = state.photos.find((p) => !p.archived && p.id !== photo.id);
    setActive(next?.id || null);
    persist();
    renderLine();
    renderCollections();
    toast("洗好了，已经挂上晾片绳。");
  }, 450);
}
async function exportNotes() {
  if (!state.notes.length) {
    toast("便签册还是空的，先留下第一张便签吧。");
    return;
  }
  try {
    const path = await invoke("export_file", {
      content: toMarkdown(state.notes, state.photos),
      kind: "md",
    });
    if (path)
      toast("便签已导出到「下载」文件夹。", false, {
        label: "打开",
        run: () => invoke("open_downloads"),
      });
  } catch (e) {
    toast("导出失败：" + String(e), true);
  }
}
async function backup() {
  try {
    const path = await invoke("export_file", {
      content: JSON.stringify(
        { ...state, exportedAt: new Date().toISOString() },
        null,
        2,
      ),
      kind: "json",
    });
    if (path)
      toast("照片和便签已完整备份到「下载」文件夹。", false, {
        label: "打开",
        run: () => invoke("open_downloads"),
      });
  } catch (e) {
    toast("备份失败：" + String(e), true);
  }
}
async function importBackup(file) {
  if (!file) return;
  try {
    if (file.size > 180_000_000) throw Error("备份文件过大");
    const data = validateCollection(JSON.parse(await file.text()));
    const photoIds = new Set(state.photos.map((p) => p.id)),
      noteIds = new Set(state.notes.map((n) => n.id));
    let photos = 0,
      notes = 0;
    for (const p of data.photos)
      if (!photoIds.has(p.id)) {
        state.photos.push(p);
        photoIds.add(p.id);
        photos++;
      }
    for (const n of data.notes)
      if (!noteIds.has(n.id)) {
        state.notes.push(n);
        noteIds.add(n.id);
        notes++;
      }
    state.photos.sort((a, b) => b.createdAt - a.createdAt);
    state.notes.sort((a, b) => b.createdAt - a.createdAt);
    if (!current()) setActive(state.photos[0]?.id || null);
    persist();
    renderNotes();
    renderLine();
    renderCollections();
    toast(`已经合并 ${photos} 张照片、${notes} 张便签。相同的收藏不会重复。`);
  } catch (e) {
    toast("导入失败：" + String(e), true);
  }
}
function dismissTip() {
  $("#welcome-tip").hidden = true;
  localStorage.setItem("darkroom-intro", "seen");
}
function showDemo() {
  const canvas = document.createElement("canvas");
  canvas.width = 1000;
  canvas.height = 650;
  const c = canvas.getContext("2d");
  c.fillStyle = "#faf3dd";
  c.fillRect(0, 0, 1000, 650);
  c.fillStyle = "#62755d";
  c.font = "20px Consolas";
  c.fillText("A SLOW WEEKEND  /  DEMO CAPTURE", 65, 65);
  c.font = 'bold 56px "Microsoft YaHei", sans-serif';
  c.fillText("周末的小小计划", 65, 165);
  c.fillStyle = "#8d9b76";
  c.fillRect(65, 200, 110, 4);
  c.fillStyle = "#59674f";
  c.font = '29px "Microsoft YaHei", sans-serif';
  c.fillText("去公园走走，收集一点秋天。", 65, 277);
  c.fillText("给喜欢的人写一封信。", 65, 339);
  c.fillText("把灵感留在这里。", 65, 401);
  c.fillStyle = "#a48657";
  c.font = "24px Consolas";
  c.fillText("https://example.com/weekend", 65, 493);
  c.fillStyle = "#b1ae91";
  c.font = '21px "Microsoft YaHei", sans-serif';
  c.fillText("这是一张试洗样片，可以自由框选文字。", 65, 581);
  c.fillStyle = "#df9974";
  c.beginPath();
  c.arc(840, 130, 76, 0, Math.PI * 2);
  c.fill();
  c.strokeStyle = "#819475";
  c.lineWidth = 6;
  c.beginPath();
  c.moveTo(817, 448);
  c.quadraticCurveTo(790, 315, 854, 235);
  c.stroke();
  for (let i = 0; i < 4; i++) {
    c.save();
    c.translate(810 + i * 5, 370 - i * 30);
    c.rotate(i % 2 ? -0.6 : 0.6);
    c.fillStyle = i % 2 ? "#9aab84" : "#b3bc95";
    c.beginPath();
    c.ellipse(i % 2 ? -23 : 23, 0, 35, 13, 0, 0, Math.PI * 2);
    c.fill();
    c.restore();
  }
  addPhoto(canvas.toDataURL("image/png"), "周末的小小计划 · 试洗样片", {
    demo: true,
  })
    .then(() => toast("试洗样片入水了。框选一句话，再点「显影文字」。"))
    .catch((e) => toast(String(e), true));
}

// Crop coordinates are always relative to the rendered image, never the tray.
const wrap = $("#image-wrap");
function pointerPosition(e) {
  const r = wrap.getBoundingClientRect();
  return {
    x: Math.max(0, Math.min(e.clientX - r.left, r.width)),
    y: Math.max(0, Math.min(e.clientY - r.top, r.height)),
  };
}
wrap.addEventListener("pointerdown", (e) => {
  if (e.button !== 0) return;
  clearSelection();
  selectionOrigin = pointerPosition(e);
  selectionDisplay = wrap.getBoundingClientRect();
  wrap.setPointerCapture(e.pointerId);
  e.preventDefault();
});
wrap.addEventListener("pointermove", (e) => {
  if (!selectionOrigin) return;
  const b = wrap.getBoundingClientRect();
  selection = cropRect(selectionOrigin, pointerPosition(e), b.width, b.height);
  const el = $("#selection");
  el.hidden = false;
  Object.assign(el.style, {
    left: selection.x + "px",
    top: selection.y + "px",
    width: selection.w + "px",
    height: selection.h + "px",
  });
  const n = naturalCrop(
    selection,
    b.width,
    b.height,
    $("#source-image").naturalWidth,
    $("#source-image").naturalHeight,
  );
  $("#selection-size").textContent = n.w + " × " + n.h;
});
function endSelection() {
  if (!selectionOrigin) return;
  selectionOrigin = null;
  if (!selection || selection.w < 5 || selection.h < 5) {
    clearSelection();
    return;
  }
  $("#clear-selection").hidden = false;
  $("#selection-hint").textContent = "选区已就绪 · 点「显影文字」提取内容";
}
wrap.addEventListener("pointerup", endSelection);
wrap.addEventListener("pointercancel", endSelection);
new ResizeObserver(() => {
  if (selection && !selectionOrigin) clearSelection();
}).observe(wrap);

$$(".tab").forEach((b) =>
  b.addEventListener("click", () => setView(b.dataset.view)),
);
$$(".filter").forEach((b) =>
  b.addEventListener("click", () => {
    filter = b.dataset.filter;
    $$(".filter").forEach((x) => x.classList.toggle("active", x === b));
    renderNotes();
  }),
);
$$(".note-type").forEach((b) =>
  b.addEventListener("click", () => setNoteType(b.dataset.type)),
);
$("#search-input").addEventListener("input", (e) => {
  query = e.target.value;
  renderNotes();
});
$("#collection-search").addEventListener("input", renderCollections);
$("#file-input").addEventListener("change", (e) => {
  importImages([...e.target.files]);
  e.target.value = "";
});
$("#backup-input").addEventListener("change", (e) => {
  importBackup(e.target.files[0]);
  e.target.value = "";
});
["#import-button", "#album-import"].forEach((s) =>
  $(s).addEventListener("click", () => $("#file-input").click()),
);
$("#demo-button").addEventListener("click", showDemo);
$("#manual-note").addEventListener("click", () => openNote());
$("#clear-selection").addEventListener("click", clearSelection);
$("#recognize-button").addEventListener("click", recognize);
$("#archive-button").addEventListener("click", archive);
$("#note-form").addEventListener("submit", saveNote);
$("#close-dialog").addEventListener("click", closeNote);
$("#note-dialog").addEventListener("cancel", (e) => {
  e.preventDefault();
  closeNote();
});
$("#note-dialog").addEventListener("click", (e) => {
  if (e.target === $("#note-dialog")) {
    const r = e.target.getBoundingClientRect();
    if (
      e.clientX < r.left ||
      e.clientX > r.right ||
      e.clientY < r.top ||
      e.clientY > r.bottom
    )
      closeNote();
  }
});
$("#export-button").addEventListener("click", exportNotes);
$("#all-export").addEventListener("click", exportNotes);
$("#view-album").addEventListener("click", () => setView("album"));
$("#settings-button").addEventListener("click", () =>
  $("#settings-dialog").showModal(),
);
$("#close-settings").addEventListener("click", () =>
  $("#settings-dialog").close(),
);
$("#backup-export").addEventListener("click", backup);
$("#backup-import").addEventListener("click", () => $("#backup-input").click());
$("#downloads-button").addEventListener("click", () =>
  invoke("open_downloads").catch((e) => toast(String(e), true)),
);
$("#data-directory-button").addEventListener("click",()=>invoke("open_data_directory").catch(error=>toast(String(error),true)));
$("#ocr-language").addEventListener("change", (e) =>
  localStorage.setItem("darkroom-language", e.target.value),
);
$("#monitor-toggle").addEventListener("click", async () => {
  const enabled = $("#monitor-toggle").getAttribute("aria-pressed") !== "true";
  try {
    await invoke("set_monitor", { enabled });
    updateMonitor(enabled);
    toast(
      enabled
        ? "自动接收已开启，下一张截图会直接挂到桌面。"
        : "已暂停自动接收。仍可手动拖入或粘贴图片。",
    );
  } catch (e) {
    toast(String(e), true);
  }
});
$("#hide").addEventListener("click", () => invoke("hide_window"));
$("#minimize").addEventListener("click", () => invoke("minimize_window"));
$("#quit-button").addEventListener("click", async () => {
  clearTimeout(saveTimer);
  await saveQueue.catch(() => {});
  if (storageHealthy)
    try {
      await invoke("save_state", { data: state });
    } catch (e) {
      toast("保存失败：" + String(e), true);
      return;
    }
  await invoke("exit_app");
});
$("#dismiss-tip").addEventListener("click", dismissTip);
$("#snip-button").addEventListener("click", () =>
  invoke("start_snip").catch((e) => toast(String(e), true)),
);
document.addEventListener("paste", async (e) => {
  if (e.target.matches("input,textarea")) return;
  const files = [...(e.clipboardData?.files || [])].filter((f) =>
    f.type.startsWith("image/"),
  );
  if (files.length) {
    e.preventDefault();
    await importImages(files);
  } else if (native) {
    try {
      const url = await invoke("paste_image");
      if (url) {
        await addPhoto(url, "粘贴的截图", { dedupe: true });
        toast("图片入水了。");
      } else toast("剪贴板中没有图片。试试 Win + Shift + S。");
    } catch (error) {
      toast(String(error), true);
    }
  }
});
let dragDepth = 0;
document.addEventListener("dragenter", (e) => {
  if ([...e.dataTransfer.types].includes("Files")) {
    e.preventDefault();
    dragDepth++;
    setView("workbench");
    $("#drag-overlay").hidden = false;
  }
});
document.addEventListener("dragover", (e) => {
  if ([...e.dataTransfer.types].includes("Files")) {
    e.preventDefault();
    e.dataTransfer.dropEffect = "copy";
  }
});
document.addEventListener("dragleave", (e) => {
  if ([...e.dataTransfer.types].includes("Files")) {
    dragDepth--;
    if (dragDepth <= 0) $("#drag-overlay").hidden = true;
  }
});
document.addEventListener("drop", (e) => {
  e.preventDefault();
  dragDepth = 0;
  $("#drag-overlay").hidden = true;
  importImages([...e.dataTransfer.files]);
});
document.addEventListener("keydown", (e) => {
  if (e.target.matches("input,textarea,select") || $("dialog[open]")) return;
  if (e.key === "/") {
    e.preventDefault();
    if (view === "notes") $("#collection-search").focus();
    else {
      setView("workbench");
      $("#search-input").focus();
    }
  }
  if (e.key === "Escape") clearSelection();
});
window.addEventListener("beforeunload", () => clearTimeout(saveTimer));

async function init() {
  renderNotes();
  renderLine();
  if (!native) {
    $("#save-status").textContent = "界面预览 · 自动接收和识别请使用桌面版";
    return;
  }
  // Keep Chromium from suspending the hidden screenshot receiver after idle.
  navigator.locks?.request("darkroom-background-capture", () => new Promise(() => {})).catch(() => {});
  try {
    const saved = await invoke("load_state");
    if (saved) {
      Object.assign(state, validateCollection(saved));
      setActive(
        state.photos.find((p) => !p.archived)?.id ||
          state.photos[0]?.id ||
          null,
      );
    }
  } catch (e) {
    storageHealthy = false;
    $("#save-status").textContent = "收藏读取失败 · 原文件已保留";
    toast(String(e), true);
  }
  renderNotes();
  renderLine();
  updateMonitor(await invoke("get_monitor"));
  updateDesktopLine(await invoke("get_desktop_line"));
  await native.event.listen("desktop-line-changed", event => updateDesktopLine(event.payload));
  await native.event.listen("desktop-line-error", event => toast(String(event.payload), true));
  await native.event.listen("open-photo", event => {
    if (state.photos.some(photo => photo.id === event.payload)) {
      setActive(event.payload);
      setView("workbench");
    } else toast("这张截图已移走，可到晾片夹查看其他照片。");
  });
  await native.event.listen("capture-ready", queueCapture);
  await native.event.listen("capture-error", (event) =>
    toast(String(event.payload), true),
  );
  await native.event.listen("monitor-changed", (event) =>
    updateMonitor(event.payload),
  );
  queueCapture();
  try {
    const langs = await invoke("ocr_languages");
    for (const lang of langs) {
      const option = element("option", "", lang);
      option.value = lang;
      $("#ocr-language").append(option);
    }
    $("#ocr-language").value =
      localStorage.getItem("darkroom-language") || "auto";
    if (!langs.length)
      toast(
        "系统尚未安装离线识别语言。可在设置中查看说明，或先手写便签。",
        true,
      );
  } catch (e) {
    toast(String(e), true);
  }
  if (!localStorage.getItem("darkroom-intro"))
    setTimeout(() => ($("#welcome-tip").hidden = false), 1800);
}
init().catch((e) => toast("初始化失败：" + String(e), true));
