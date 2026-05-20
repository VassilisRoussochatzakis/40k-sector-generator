(function(){
"use strict";
// Pointy-top, odd-r offset hex layout — mirrors src/bitmap/mod.rs::hex_center.
const HEX_SIZE_BASE = 26;
const STAR_R_RATIO = 0.2016;

const canvas = document.getElementById("map");
const ctx = canvas.getContext("2d");
const panel = document.getElementById("panel");
const heatSel = document.getElementById("heat");
const factionFillBox = document.getElementById("factionFill");
const showRoutesBox = document.getElementById("showRoutes");
const showLabelsBox = document.getElementById("showLabels");
const filterDiv = document.getElementById("filter");

const STAR_COLORS = {O:"#ff9646",B:"#b4d2ff",A:"#ffc85a",F:"#dc5ac8",G:"#6ed282",K:"#c8be82",M:"#c83c46"};
const STABILITY_COLORS = {stable:"#6ed282",unstable:"#f0c85a",hazardous:"#eb5a5a",perilous:"#a564d7"};

// View state.
const state = {
  scale: 1.0,
  tx: 0, ty: 0,
  hexSize: HEX_SIZE_BASE,
  selectedSystem: null,
  heat: "off",
  factionFill: true,
  showRoutes: true,
  showLabels: true,
  hiddenFactions: new Set(),
};

// Precomputed system → world counts, per-faction lookups.
const systemById = {};
for (const s of SECTOR.systems) systemById[s.id] = s;
const factionById = {};
for (const f of SECTOR.factions) factionById[f.id] = f;

// ── Geometry ───────────────────────────────────────────────────────────────
function hexCenter(q, r){
  const hs = state.hexSize;
  const horiz = hs * Math.sqrt(3);
  const vert = hs * 1.5;
  const shift = (r & 1) === 0 ? 0.0 : 0.5;
  const margin = 28;
  return [margin + horiz * (q + shift) + horiz/2, margin + vert*r + hs];
}
function hexCorner(cx, cy, i){
  const a = Math.PI/180 * (60*i - 30);
  return [cx + state.hexSize*Math.cos(a), cy + state.hexSize*Math.sin(a)];
}
function pixelToOddR(x, y){
  // Inverse via brute force: find the system whose center is closest within
  // hex_size — the grid is small so this is fast and avoids edge cases on
  // odd-r boundaries.
  let best=null, bestD=Infinity;
  for (const s of SECTOR.systems){
    const [cx, cy] = hexCenter(s.coord.q, s.coord.r);
    const d = (x-cx)*(x-cx) + (y-cy)*(y-cy);
    if (d < bestD){ bestD = d; best = s; }
  }
  if (best === null) return null;
  return bestD <= state.hexSize*state.hexSize ? best : null;
}

// ── Heatmap (client-side, on already-derived sector fields) ────────────────
function heatScore(sys, mode){
  if (mode === "off") return [0, null];
  if (mode === "control"){
    const dom = sys.control && sys.control.dominant;
    return [dom ? 1 : 0, dom || null];
  }
  // Pull per-system aggregates the model already exposes. Conservative —
  // unknown fields default to 0.
  const pop = sys.worlds.reduce((a,w)=>a+(w.factions||[]).length, 0);
  if (mode === "worlds") return [Math.min(1, sys.worlds.length/6), null];
  if (mode === "presences") return [Math.min(1, pop/12), null];
  if (mode === "factions") return [Math.min(1, (sys.primary_factions||[]).length/4), null];
  return [0, null];
}
function heatColor(mode, intensity){
  const base = {
    worlds:[110,210,130],
    presences:[235,90,90],
    factions:[150,90,220],
    control:[80,200,255],
  }[mode] || [120,200,240];
  const strength = 0.18 + intensity*0.55;
  const mix = (v) => Math.round(v*strength + parseInt(THEME.hex.slice(1,3),16)*(1-strength));
  return `rgb(${mix(base[0])},${mix(base[1])},${mix(base[2])})`;
}

// ── Rendering ──────────────────────────────────────────────────────────────
function resize(){
  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.parentElement.getBoundingClientRect();
  canvas.width = rect.width * dpr;
  canvas.height = rect.height * dpr;
  canvas.style.width = rect.width + "px";
  canvas.style.height = rect.height + "px";
  ctx.setTransform(dpr,0,0,dpr,0,0);
  draw();
}

function draw(){
  ctx.save();
  ctx.fillStyle = THEME.bg;
  ctx.fillRect(0,0,canvas.width,canvas.height);
  ctx.translate(state.tx, state.ty);
  ctx.scale(state.scale, state.scale);

  // Empty hex grid first.
  const sysSet = new Map();
  for (const s of SECTOR.systems) sysSet.set(s.coord.q+","+s.coord.r, s);
  for (let r=0; r<SECTOR.height; r++){
    for (let q=0; q<SECTOR.width; q++){
      const [cx,cy] = hexCenter(q,r);
      const sys = sysSet.get(q+","+r);
      let fill = THEME.hex;
      if (sys){
        const [intensity, dom] = heatScore(sys, state.heat);
        if (state.heat !== "off" && intensity > 0){
          fill = heatColor(state.heat, intensity);
        } else if (state.factionFill && sys.control && sys.control.dominant){
          const pal = FACTION_PALETTE[sys.control.dominant];
          if (pal) fill = mixHex(pal.fill, THEME.hex, 0.4);
        }
      }
      drawHex(cx, cy, fill, THEME.outline);
    }
  }

  // Routes.
  if (state.showRoutes){
    ctx.lineWidth = Math.max(2, state.hexSize*0.08);
    for (const r of SECTOR.routes){
      const a = systemById[r.from_system_id];
      const b = systemById[r.to_system_id];
      if (!a || !b) continue;
      if (factionFiltered(a) || factionFiltered(b)) continue;
      const [ax,ay] = hexCenter(a.coord.q, a.coord.r);
      const [bx,by] = hexCenter(b.coord.q, b.coord.r);
      ctx.strokeStyle = STABILITY_COLORS[r.stability] || THEME.text;
      ctx.setLineDash(dashesForRoute(r));
      ctx.beginPath();
      ctx.moveTo(ax,ay); ctx.lineTo(bx,by); ctx.stroke();
    }
    ctx.setLineDash([]);
  }

  // Systems + labels.
  const starR = state.hexSize * STAR_R_RATIO;
  for (const s of SECTOR.systems){
    if (factionFiltered(s)) continue;
    const [cx,cy] = hexCenter(s.coord.q, s.coord.r);
    const sc = STAR_COLORS[s.star.colour_code] || "#b4b4b4";
    ctx.fillStyle = sc;
    ctx.beginPath(); ctx.arc(cx,cy,starR,0,Math.PI*2); ctx.fill();
    ctx.strokeStyle = darken(sc, 0.5);
    ctx.lineWidth = 1; ctx.stroke();
    if (state.selectedSystem === s.id){
      ctx.strokeStyle = THEME.accent;
      ctx.lineWidth = 2;
      ctx.beginPath(); ctx.arc(cx,cy,starR+3,0,Math.PI*2); ctx.stroke();
    }
    if (state.showLabels){
      ctx.fillStyle = THEME.dim;
      ctx.font = (Math.max(8, state.hexSize*0.3)|0) + "px ui-monospace,monospace";
      ctx.textAlign = "center";
      ctx.fillText(s.name.toUpperCase(), cx, cy + starR + state.hexSize*0.4);
    }
  }
  ctx.restore();
}

function drawHex(cx, cy, fill, outline){
  ctx.beginPath();
  for (let i=0;i<6;i++){
    const [x,y] = hexCorner(cx,cy,i);
    if (i===0) ctx.moveTo(x,y); else ctx.lineTo(x,y);
  }
  ctx.closePath();
  ctx.fillStyle = fill;
  ctx.fill();
  ctx.strokeStyle = outline;
  ctx.lineWidth = 1;
  ctx.stroke();
}

const ROUTE_TYPE_ALIASES = {
  StableWarpLane:"stable_warp_lane",
  ChartedPassage:"charted_passage",
  DangerousPassage:"dangerous_passage",
  SecretPassage:"secret_passage",
  Webway:"webway",
  BlackShip:"black_ship",
  SmugglingLane:"smuggling_lane",
};
const ROUTE_PATTERN_POOLS = {
  stable_warp_lane:["Solid","Railroad","March"],
  charted_passage:["Dashed","Bridge","Twin"],
  dangerous_passage:["DotDash","Cracked","Staccato"],
  secret_passage:["Dotted","Tick","Whisper"],
  webway:["Burst","Tripod","Patter"],
  black_ship:["Quartet","DoubleTap"],
  smuggling_lane:["Gravel","Pebble","Ghost"],
};
const ROUTE_PATTERN_STRIDES = {
  Solid:[],
  Dashed:[10,5],
  DotDash:[5,2,1,2,1,4],
  Dotted:[1,2],
  Cracked:[3,2],
  Ghost:[12,15],
  Burst:[1.5,2,1.5,2,1.5,8],
  Staccato:[6,3,2,3],
  Gravel:[2,1.5],
  Twin:[4,2,4,5],
  Tripod:[6,1,1,1,1,1,6],
  Tick:[2,8],
  Bridge:[4,2,4,2],
  Patter:[0.8,1.2],
  Quartet:[5,3,3,7],
  Railroad:[14,6],
  DoubleTap:[2.5,2,2.5,6],
  Pebble:[1,1],
  Whisper:[1,14],
  March:[3,3,3,3,3,3],
};
const ROUTE_TEXT_ENCODER = new TextEncoder();

function dashesForRoute(route){
  const routeType = routeTypeKey(route.route_type);
  const pool = ROUTE_PATTERN_POOLS[routeType] || ROUTE_PATTERN_POOLS.charted_passage;
  const salt = SECTOR.seed || SECTOR.id || "";
  const key = [
    salt,
    route.id || "",
    route.from_system_id || "",
    route.to_system_id || "",
    route.distance || 0,
    stabilityKey(route.stability),
  ].join("\0");
  const pattern = pool[stableRoutePatternHash(routeType, key) % pool.length];
  const unit = Math.max(2, ctx.lineWidth || 2);
  return (ROUTE_PATTERN_STRIDES[pattern] || []).map(v => v * unit);
}

function routeTypeKey(routeType){
  const raw = String(routeType || "charted_passage");
  return ROUTE_TYPE_ALIASES[raw] || raw;
}

function stabilityKey(stability){
  return String(stability || "stable").toLowerCase();
}

function stableRoutePatternHash(routeType, key){
  let hash = 2166136261 >>> 0;
  hash = fnvFeed(hash, "sectorforge:route-pattern:v1");
  hash = fnvFeedByte(hash, 0);
  hash = fnvFeed(hash, routeType);
  hash = fnvFeedByte(hash, 0);
  hash = fnvFeed(hash, key);
  return hash >>> 0;
}

function fnvFeed(hash, text){
  const bytes = ROUTE_TEXT_ENCODER.encode(text);
  for (const b of bytes) hash = fnvFeedByte(hash, b);
  return hash >>> 0;
}

function fnvFeedByte(hash, byte){
  hash ^= byte;
  return Math.imul(hash, 16777619) >>> 0;
}

function darken(hex, amt){
  const [r,g,b] = parseHex(hex);
  const k = 1 - amt;
  return `rgb(${(r*k)|0},${(g*k)|0},${(b*k)|0})`;
}
function parseHex(h){
  const v = h.startsWith("rgb") ? h.match(/\d+/g).map(Number) : [
    parseInt(h.slice(1,3),16), parseInt(h.slice(3,5),16), parseInt(h.slice(5,7),16)
  ];
  return v;
}
function mixHex(a, b, t){
  const [ar,ag,ab] = parseHex(a);
  const [br,bg,bb] = parseHex(b);
  const m = (x,y) => Math.round(x*t + y*(1-t));
  return `rgb(${m(ar,br)},${m(ag,bg)},${m(ab,bb)})`;
}

function factionFiltered(sys){
  if (state.hiddenFactions.size === 0) return false;
  const fids = new Set([...(sys.primary_factions||[]), sys.control && sys.control.dominant].filter(Boolean));
  if (fids.size === 0) return false;
  // Hide the system only when *every* faction touching it is hidden.
  for (const f of fids) if (!state.hiddenFactions.has(f)) return false;
  return true;
}

// ── Side panel ─────────────────────────────────────────────────────────────
function selectSystem(id){
  state.selectedSystem = id;
  const sys = systemById[id];
  if (!sys){ panel.innerHTML = "Click a system."; draw(); return; }
  let html = "";
  html += `<h2>${esc(sys.name)} <span class=badge>${esc(sys.id)}</span></h2>`;
  html += `<div class=row><span class=key>coord</span><span>${sys.coord.q},${sys.coord.r}</span></div>`;
  html += `<div class=row><span class=key>star</span><span>${esc(sys.star.colour_name)} (${esc(sys.star.colour_code)})</span></div>`;
  if (sys.control && sys.control.dominant){
    const f = factionById[sys.control.dominant];
    html += `<div class=row><span class=key>dominant</span><span>${esc(f ? f.name : sys.control.dominant)}</span></div>`;
  }
  if (sys.tags && sys.tags.length) html += `<div class=row><span class=key>tags</span><span>${sys.tags.map(esc).join(", ")}</span></div>`;
  html += `<h3>Worlds (${sys.worlds.length})</h3><ul>`;
  for (const w of sys.worlds){
    html += `<li><strong>${esc(w.name)}</strong> — ${esc(w.world.world_type)} / ${esc(w.world.population)}`;
    if (w.factions && w.factions.length){
      html += " <span class=key>(" + w.factions.map(p => esc(presenceLabel(p))).join(", ") + ")</span>";
    }
    html += "</li>";
  }
  html += "</ul>";
  if (sys.primary_factions && sys.primary_factions.length){
    html += "<h3>Primary factions</h3><ul>";
    for (const fid of sys.primary_factions){
      const f = factionById[fid];
      const pal = FACTION_PALETTE[fid];
      const sw = pal ? `<span class=sw style="background:${pal.fill}"></span>` : "";
      html += `<li>${sw}${esc(f ? f.name : fid)}</li>`;
    }
    html += "</ul>";
  }
  panel.innerHTML = html;
  draw();
}
function esc(s){ return String(s).replace(/[&<>"]/g, c => ({"&":"&amp;","<":"&lt;",">":"&gt;","\"":"&quot;"}[c])); }
function presenceLabel(p){
  const sub = p.subfaction_name || p.subfaction_id || "";
  return sub ? `${p.faction_id}: ${sub}` : p.faction_id;
}

// ── Controls ───────────────────────────────────────────────────────────────
function buildHeatOptions(){
  const opts = [
    ["off","OFF"],["control","CONTROL"],["worlds","WORLDS"],
    ["presences","PRESENCES"],["factions","FACTIONS"],
  ];
  heatSel.innerHTML = "";
  for (const [v,l] of opts){
    const o = document.createElement("option");
    o.value = v; o.textContent = l;
    heatSel.appendChild(o);
  }
}
function buildFactionFilter(){
  filterDiv.innerHTML = "";
  const ids = Object.keys(FACTION_PALETTE).sort();
  for (const fid of ids){
    const pal = FACTION_PALETTE[fid];
    const el = document.createElement("span");
    el.className = "chip on";
    el.dataset.fid = fid;
    el.innerHTML = `<span class="sw" style="background:${pal.fill}"></span>${esc(pal.name)}`;
    el.addEventListener("click", () => {
      if (state.hiddenFactions.has(fid)){
        state.hiddenFactions.delete(fid);
        el.classList.add("on");
      } else {
        state.hiddenFactions.add(fid);
        el.classList.remove("on");
      }
      draw();
    });
    filterDiv.appendChild(el);
  }
}

// ── Interaction ────────────────────────────────────────────────────────────
let dragging = false, lastX = 0, lastY = 0, dragMoved = false;
canvas.addEventListener("mousedown", e => {
  dragging = true; dragMoved = false; lastX = e.clientX; lastY = e.clientY;
  canvas.classList.add("dragging");
});
window.addEventListener("mouseup", e => {
  if (!dragging) return;
  dragging = false;
  canvas.classList.remove("dragging");
  if (!dragMoved){
    const rect = canvas.getBoundingClientRect();
    const x = (e.clientX - rect.left - state.tx) / state.scale;
    const y = (e.clientY - rect.top  - state.ty) / state.scale;
    const sys = pixelToOddR(x, y);
    if (sys) selectSystem(sys.id);
  }
});
window.addEventListener("mousemove", e => {
  if (!dragging) return;
  const dx = e.clientX - lastX, dy = e.clientY - lastY;
  if (Math.abs(dx) + Math.abs(dy) > 3) dragMoved = true;
  state.tx += dx; state.ty += dy;
  lastX = e.clientX; lastY = e.clientY;
  draw();
});
canvas.addEventListener("wheel", e => {
  e.preventDefault();
  const rect = canvas.getBoundingClientRect();
  const mx = e.clientX - rect.left, my = e.clientY - rect.top;
  const factor = e.deltaY < 0 ? 1.15 : 1/1.15;
  const newScale = Math.max(0.2, Math.min(6, state.scale * factor));
  state.tx = mx - (mx - state.tx) * (newScale / state.scale);
  state.ty = my - (my - state.ty) * (newScale / state.scale);
  state.scale = newScale;
  draw();
}, {passive: false});

heatSel.addEventListener("change", () => { state.heat = heatSel.value; draw(); });
factionFillBox.addEventListener("change", () => { state.factionFill = factionFillBox.checked; draw(); });
showRoutesBox.addEventListener("change", () => { state.showRoutes = showRoutesBox.checked; draw(); });
showLabelsBox.addEventListener("change", () => { state.showLabels = showLabelsBox.checked; draw(); });
window.addEventListener("resize", resize);

// ── Init ───────────────────────────────────────────────────────────────────
buildHeatOptions();
buildFactionFilter();
// Centre + fit on first render.
(function fit(){
  const rect = canvas.parentElement.getBoundingClientRect();
  const horiz = state.hexSize * Math.sqrt(3);
  const vert = state.hexSize * 1.5;
  const w = 56 + horiz * (SECTOR.width + 0.5);
  const h = 56 + (SECTOR.height-1) * vert + 2*state.hexSize;
  const sx = rect.width / w, sy = rect.height / h;
  state.scale = Math.min(sx, sy) * 0.92;
  state.tx = (rect.width  - w*state.scale)/2;
  state.ty = (rect.height - h*state.scale)/2;
})();
resize();
})();
