const native = window.__TAURI__;
const invoke = (command, args = {}) => native.core.invoke(command, args);
const $ = (selector) => document.querySelector(selector);
const status = (message, error = false) => {
  $("#widget-status").textContent = message;
  $("#widget-status").classList.toggle("error", error);
};
let lastSignature = "", refreshQueue = Promise.resolve(), monitorEnabled = true;
async function refresh() {
  const photos = await invoke("desktop_photos");
  const signature = JSON.stringify(photos.map(({id, name, archived,pinned}) => [id, name, archived,pinned]));
  if (signature === lastSignature) return;
  lastSignature = signature;
  const line = $("#desktop-photos");
  line.replaceChildren();
  const pins=photos.filter(photo=>photo.pinned).length;
  $("#count").textContent = photos.length ? `${photos.length} 张${pins?` · ${pins} 张固定`:" · 点击整理"}` : "等一张新照片";
  if (!photos.length) {
    for (const [index, text] of ["第一张新照片", "一句有用的话", "一点小小灵感"].entries()) {
      const card = document.createElement("div");
      card.className = "desktop-photo desktop-placeholder";
      card.style.setProperty("--tilt", [-3, 4, -2][index]+"deg");
      card.append(document.createTextNode(["▧", "≋", "✳"][index]));
      const caption = document.createElement("span");caption.textContent = text;card.append(caption);line.append(card);
    }
  }
  photos.forEach((photo, index) => {
    const card = document.createElement("button");card.className = "desktop-photo";
    card.style.setProperty("--tilt", [-4, 3, -2, 4, -3, 2][index%6]+"deg");
    card.title = "打开："+photo.name;card.setAttribute("aria-label", "打开截图："+photo.name);
    const image = document.createElement("img");image.src = photo.dataUrl;image.alt = photo.name;image.draggable = false;
    const caption = document.createElement("span");caption.className = "caption";
    caption.textContent = photo.pinned ? "★ 已固定" : photo.archived ? "✓ 已晾好" : new Date(photo.createdAt).toLocaleTimeString("zh-CN",{hour:"2-digit",minute:"2-digit"})+" · 等待显影";
    card.append(image, caption);card.addEventListener("click", () => invoke("open_desktop_photo", {id:photo.id}).catch(error=>status(String(error),true)));
    line.append(card);
  });
  status(monitorEnabled?"把有用的瞬间，晾在眼前。":"已暂停自动接收 · 点击 ▶ 继续");
  await Promise.all([...document.querySelectorAll(".desktop-photo img")].map(image=>image.decode().catch(()=>{})));
  await invoke("desktop_line_ready",{layout:{width:window.innerWidth,height:window.innerHeight,cards:document.querySelectorAll(".desktop-photo").length,heading:document.querySelector(".room-name").textContent,bodyTextLength:document.body.innerText.length}});
}
function queueRefresh() { refreshQueue = refreshQueue.catch(()=>{}).then(refresh).catch(error=>status("暂时无法读取照片："+String(error),true)); return refreshQueue; }
$("#open-room").addEventListener("click", () => invoke("open_desktop_photo",{id:null}));
$("#hide-line").addEventListener("click", () => invoke("close_desktop_line").catch(error=>status(String(error),true)));
function updateMonitor(enabled) {
  monitorEnabled=enabled;const button=$("#pause-capture");
  button.textContent=enabled?"Ⅱ":"▶";button.title=enabled?"暂停自动接收":"恢复自动接收";
  button.setAttribute("aria-label",button.title);button.setAttribute("aria-pressed",String(!enabled));
  status(enabled?"把有用的瞬间，晾在眼前。":"已暂停自动接收 · 点击 ▶ 继续");
}
$("#pause-capture").addEventListener("click",async()=>{
  try {await invoke("set_monitor",{enabled:!monitorEnabled});}catch(error){status(String(error),true);}
});

// Screen coordinates stay stable while the child window moves with the pointer.
let drag = null, moveQueue = Promise.resolve();
const handle = $("#move-handle");
handle.addEventListener("pointerdown", event => {
  if(event.button !== 0 || event.target.closest("button")) return;
  const session = {startX:event.screenX,startY:event.screenY,screenX:event.screenX,screenY:event.screenY,position:null,ended:false};
  drag=session;handle.setPointerCapture(event.pointerId);event.preventDefault();
  invoke("desktop_line_position").then(position=>{session.position=position;move(session,session.ended);}).catch(error=>status(String(error),true));
});
function move(session, save) {
  if(!session.position)return;
  const [x,y,scale]=session.position;
  const target = {x:Math.round(x+(session.screenX-session.startX)*scale),y:Math.round(y+(session.screenY-session.startY)*scale),save};
  moveQueue=moveQueue.catch(()=>{}).then(()=>invoke("move_desktop_line",target)).catch(error=>status(String(error),true));
}
handle.addEventListener("pointermove",event=>{
  if(!drag)return;drag.screenX=event.screenX;drag.screenY=event.screenY;move(drag,false);
});
function endDrag(event) {
  if(!drag)return;const session=drag;drag=null;session.ended=true;
  if(event.type !== "pointercancel"){session.screenX=event.screenX;session.screenY=event.screenY;}move(session,true);
}
handle.addEventListener("pointerup",endDrag);
handle.addEventListener("pointercancel",endDrag);
async function init() {
  if(!native){status("桌面挂件请使用桌面版打开。");return;}
  updateMonitor(await invoke("get_monitor"));
  await native.event.listen("monitor-changed",event=>updateMonitor(event.payload));
  await native.event.listen("desktop-photos-changed",queueRefresh);
  await native.event.listen("desktop-line-changed",event=>{if(event.payload.enabled)queueRefresh();});
  await queueRefresh();
}
init().catch(error=>status(String(error),true));
