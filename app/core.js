export const TYPES = {
  text: { label: "摘录", icon: "note" },
  link: { label: "链接", icon: "link" },
  todo: { label: "待办", icon: "check" },
};
export function normalizeOcrText(text) {
  return text.replace(/([\p{Script=Han}])[ \t]+(?=[\p{Script=Han}])/gu, "$1");
}
export function selectClotheslinePhotos(photos, limit = 6) {
  return [...photos].sort((a,b)=>Number(!!b.pinned)-Number(!!a.pinned)||b.createdAt-a.createdAt).slice(0,limit);
}
export function cropRect(start, end, width, height) {
  const clamp = (v, max) => Math.max(0, Math.min(v, max));
  const x = clamp(Math.min(start.x, end.x), width),
    y = clamp(Math.min(start.y, end.y), height);
  return {
    x,
    y,
    w: clamp(Math.max(start.x, end.x), width) - x,
    h: clamp(Math.max(start.y, end.y), height) - y,
  };
}
export function naturalCrop(
  rect,
  displayWidth,
  displayHeight,
  imageWidth,
  imageHeight,
) {
  if (!rect) return { x: 0, y: 0, w: imageWidth, h: imageHeight };
  const x = Math.max(0, Math.round((rect.x / displayWidth) * imageWidth)),
    y = Math.max(0, Math.round((rect.y / displayHeight) * imageHeight));
  return {
    x,
    y,
    w: Math.min(
      imageWidth - x,
      Math.max(1, Math.round((rect.w / displayWidth) * imageWidth)),
    ),
    h: Math.min(
      imageHeight - y,
      Math.max(1, Math.round((rect.h / displayHeight) * imageHeight)),
    ),
  };
}
export function getLinks(text) {
  return [
    ...new Set(
      (text.match(/https?:\/\/[^\s<>"'，。；、）)]+/gi) || []).map((s) =>
        s.replace(/[.,;!?]+$/, ""),
      ),
    ),
  ].filter((s) => {
    try {
      const u = new URL(s);
      return ["http:", "https:"].includes(u.protocol);
    } catch {
      return false;
    }
  });
}
export function matchesNote(note, query = "", filter = "all") {
  const text = [note.text, ...(note.tags || [])].join(" ").toLocaleLowerCase();
  return (
    (filter === "all" || note.type === filter) &&
    query
      .trim()
      .toLocaleLowerCase()
      .split(/\s+/)
      .every((term) => text.includes(term))
  );
}
export function toMarkdown(notes, photos = []) {
  const map = new Map(photos.map((p) => [p.id, p.name]));
  return (
    "# 截图冲洗室 · 便签册\n\n" +
    notes
      .map((n) => {
        const type = TYPES[n.type]?.label || "摘录";
        const lines = n.text.replace(/\r\n/g, "\n");
        return `## ${type} · ${new Date(n.createdAt).toLocaleDateString("zh-CN")}\n\n${
          n.type === "todo"
            ? lines
                .split("\n")
                .map((l) => `- [${n.done ? "x" : " "}] ${l}`)
                .join("\n")
            : lines
        }\n\n${n.tags?.length ? "标签：" + n.tags.map((t) => "#" + t).join(" ") + "\n\n" : ""}${map.has(n.photoId) ? "来源：" + map.get(n.photoId) + "\n\n" : ""}---`;
      })
      .join("\n\n") +
    "\n"
  );
}
export function validateCollection(data) {
  if (
    !data ||
    data.version !== 1 ||
    !Array.isArray(data.photos) ||
    !Array.isArray(data.notes) ||
    data.photos.length > 600 ||
    data.notes.length > 10000
  )
    throw Error("这不是有效的冲洗室备份");
  const ids = new Set();
  const photos = data.photos.map((p) => {
    if (
      !p ||
      typeof p.id !== "string" ||
      ids.has(p.id) ||
      typeof p.name !== "string" ||
      typeof p.dataUrl !== "string" ||
      !/^data:image\/(png|jpeg|webp|gif|bmp);base64,[A-Za-z0-9+/=]+$/.test(
        p.dataUrl,
      ) ||
      !Number.isFinite(p.createdAt)
    )
      throw Error("备份中的照片数据不完整");
    ids.add(p.id);
    return {
      id: p.id,
      name: p.name.slice(0, 200),
      dataUrl: p.dataUrl,
      createdAt: p.createdAt,
      archived: !!p.archived,
      demo: !!p.demo,
      pinned: !!p.pinned,
    };
  });
  ids.clear();
  const notes = data.notes.map((n) => {
    if (
      !n ||
      typeof n.id !== "string" ||
      ids.has(n.id) ||
      typeof n.text !== "string" ||
      n.text.length > 30000 ||
      !TYPES[n.type] ||
      !Number.isFinite(n.createdAt)
    )
      throw Error("备份中的便签数据不完整");
    ids.add(n.id);
    return {
      id: n.id,
      text: n.text,
      type: n.type,
      tags: Array.isArray(n.tags)
        ? n.tags
            .filter((t) => typeof t === "string")
            .map((t) => t.slice(0, 40))
            .slice(0, 12)
        : [],
      photoId: typeof n.photoId === "string" ? n.photoId : null,
      createdAt: n.createdAt,
      done: !!n.done,
    };
  });
  return { version: 1, photos, notes };
}
