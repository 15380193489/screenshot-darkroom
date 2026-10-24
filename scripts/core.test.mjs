import { test } from "node:test";
import assert from "node:assert/strict";
import {
  normalizeOcrText,
  selectClotheslinePhotos,
  cropRect,
  naturalCrop,
  getLinks,
  matchesNote,
  toMarkdown,
  validateCollection,
} from "../app/core.js";
test("Chinese OCR spacing is cleaned without merging paragraphs or English words", () => {
  assert.equal(normalizeOcrText("把 灵 感 留 在 这 里\nHello Darkroom\n中 文 English"), "把灵感留在这里\nHello Darkroom\n中文 English");
});
test("reverse-direction selection and high-DPI crop map to original image", () => {
  const r = cropRect({ x: 200, y: 150 }, { x: 20, y: 30 }, 300, 200);
  assert.deepEqual(r, { x: 20, y: 30, w: 180, h: 120 });
  assert.deepEqual(naturalCrop(r, 300, 200, 1800, 1200), {
    x: 120,
    y: 180,
    w: 1080,
    h: 720,
  });
  assert.deepEqual(naturalCrop(null, 300, 200, 1800, 1200), {
    x: 0,
    y: 0,
    w: 1800,
    h: 1200,
  });
});
test("links exclude script protocols, trim punctuation, and deduplicate", () => {
  assert.deepEqual(
    getLinks(
      "javascript:alert(1) https://example.com/a， https://example.com/a https://test.com.",
    ),
    ["https://example.com/a", "https://test.com"],
  );
});
test("search matches content and tags with multiple words and type filter", () => {
  const n = { text: "Weekend Notes", tags: ["公园", "秋天"], type: "text" };
  assert.equal(matchesNote(n, "week 秋天"), true);
  assert.equal(matchesNote(n, "week 春天"), false);
  assert.equal(matchesNote(n, "秋天", "todo"), false);
});
test("backup validation rejects non-image payloads and duplicate identities", () => {
  const p = {
    id: "p",
    name: "photo",
    dataUrl: "data:image/png;base64,YWJj",
    createdAt: 1,
  };
  assert.throws(() =>
    validateCollection({
      version: 1,
      photos: [{ ...p, dataUrl: "javascript:alert(1)" }],
      notes: [],
    }),
  );
  assert.throws(() =>
    validateCollection({ version: 1, photos: [p, p], notes: [] }),
  );
  assert.deepEqual(
    validateCollection({ version: 1, photos: [p], notes: [] }).photos[0],
    { ...p, archived: false, demo: false,pinned:false },
  );
});
test("fixed references take priority without mutating the album order",()=>{
  const photos=[{id:"new",createdAt:30},{id:"fixed",createdAt:1,pinned:true},{id:"mid",createdAt:20}];
  assert.deepEqual(selectClotheslinePhotos(photos,2).map(p=>p.id),["fixed","new"]);
  assert.deepEqual(photos.map(p=>p.id),["new","fixed","mid"]);
  const restored=validateCollection({version:1,photos:[{id:"p",name:"reference",dataUrl:"data:image/png;base64,YWJj",createdAt:1,pinned:true}],notes:[]});
  assert.equal(restored.photos[0].pinned,true);
});
test("Markdown export retains multiline task completion, tags and photo provenance", () => {
  const md = toMarkdown(
    [
      {
        id: "n",
        text: "第一件事\n第二件事",
        type: "todo",
        done: true,
        tags: ["灵感"],
        photoId: "p",
        createdAt: 1,
      },
    ],
    [{ id: "p", name: "试洗样片" }],
  );
  assert.match(md, /- \[x\] 第一件事\n- \[x\] 第二件事/);
  assert.match(md, /#灵感/);
  assert.match(md, /来源：试洗样片/);
});
