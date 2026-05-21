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
    const routeWidth = Math.max(2, state.hexSize*0.08);
    for (const r of SECTOR.routes){
      const a = systemById[r.from_system_id];
      const b = systemById[r.to_system_id];
      if (!a || !b) continue;
      if (factionFiltered(a) || factionFiltered(b)) continue;
      const [ax,ay] = hexCenter(a.coord.q, a.coord.r);
      const [bx,by] = hexCenter(b.coord.q, b.coord.r);
      drawRoutePattern(
        ax, ay, bx, by,
        STABILITY_COLORS[r.stability] || THEME.text,
        routeWidth,
        patternForRoute(r)
      );
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
  DangerousPassage:"charted_passage",
  dangerous_passage:"charted_passage",
  SecretPassage:"secret_passage",
  Webway:"webway",
  BlackShip:"black_ship",
  SmugglingLane:"smuggling_lane",
};
const ROUTE_PATTERN_POOLS = {
  stable_warp_lane:["Solid","Railroad","March"],
  charted_passage:["Dashed","Bridge","Twin","DotDash","Cracked","Staccato"],
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

function patternForRoute(route){
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
  return pool[stableRoutePatternHash(routeType, key) % pool.length];
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

function drawRoutePattern(x0, y0, x1, y1, color, width, pattern){
  const g = routeGeom(x0, y0, x1, y1, width);
  if (!g) return;
  switch (pattern){
    case "Solid":
      drawRouteSolid(g, color, width);
      break;
    case "Dashed":
    case "DotDash":
    case "Dotted":
      drawRouteStrides(g, color, width, ROUTE_PATTERN_STRIDES[pattern] || []);
      break;
    case "Cracked":
      drawRouteJagged(g, color, width, g.unit*3.0, width*1.7);
      break;
    case "Ghost":
      drawRouteStrides(g, color, width, ROUTE_PATTERN_STRIDES[pattern] || [], 0.45);
      break;
    case "Burst":
      drawRouteBursts(g, color, width, g.unit*5.0);
      break;
    case "Staccato":
      drawRouteZigzag(g, color, width, g.unit*3.2, width*1.8);
      break;
    case "Gravel":
      drawRouteDots(g, color, width, g.unit*1.55, false, true);
      break;
    case "Twin":
      drawRouteParallel(g, color, width, width);
      break;
    case "Tripod":
      drawRouteTriangles(g, color, width, g.unit*5.0);
      break;
    case "Tick":
      drawRouteSpine(g, color, width, 0.28);
      drawRouteTicks(g, color, width, g.unit*4.5, width*2.2);
      break;
    case "Bridge":
      drawRouteStrides(g, color, width*0.8, [3,2], 0.55);
      drawRouteTicks(g, color, width, g.unit*5.0, width*1.8);
      break;
    case "Patter":
      drawRouteDots(g, color, width, g.unit*2.2, true, false);
      break;
    case "Quartet":
      drawRouteDotClusters(g, color, width, g.unit*8.0, 4);
      break;
    case "Railroad": {
      const offset = width*1.25;
      drawRouteParallel(g, color, width*0.8, offset);
      drawRouteTicks(g, color, width*0.75, g.unit*5.5, offset*1.15);
      break;
    }
    case "DoubleTap":
      drawRouteDoubleTaps(g, color, width, g.unit*7.0);
      break;
    case "Pebble":
      drawRouteDots(g, color, width, g.unit*2.6, false, true);
      break;
    case "Whisper":
      drawRouteDots(g, color, width, g.unit*7.0, false, false, 0.7);
      break;
    case "March":
      drawRouteChevrons(g, color, width, g.unit*5.5);
      break;
    default:
      drawRouteStrides(g, color, width, ROUTE_PATTERN_STRIDES[pattern] || []);
  }
}

function routeGeom(x0, y0, x1, y1, width){
  const dx = x1-x0, dy = y1-y0;
  const total = Math.hypot(dx, dy);
  if (total <= 0) return null;
  const ux = dx/total, uy = dy/total;
  return {x0,y0,x1,y1,total,ux,uy,nx:-uy,ny:ux,unit:Math.max(2,width)};
}

function routePoint(g, t, off=0){
  const tt = Math.max(0, Math.min(g.total, t));
  return [g.x0 + g.ux*tt + g.nx*off, g.y0 + g.uy*tt + g.ny*off];
}

function withRouteStroke(color, width, alpha, fn){
  ctx.save();
  ctx.strokeStyle = color;
  ctx.fillStyle = color;
  ctx.lineWidth = Math.max(1, width);
  ctx.lineCap = "round";
  ctx.lineJoin = "round";
  ctx.globalAlpha *= alpha == null ? 1 : alpha;
  ctx.setLineDash([]);
  fn();
  ctx.restore();
}

function drawRouteSolid(g, color, width){
  withRouteStroke(color, width, 1, () => {
    ctx.beginPath();
    ctx.moveTo(g.x0,g.y0);
    ctx.lineTo(g.x1,g.y1);
    ctx.stroke();
  });
}

function drawRouteStrides(g, color, width, strides, alpha=1){
  if (!strides.length){
    withRouteStroke(color, width, alpha, () => {
      ctx.beginPath();
      ctx.moveTo(g.x0,g.y0);
      ctx.lineTo(g.x1,g.y1);
      ctx.stroke();
    });
    return;
  }
  withRouteStroke(color, width, alpha, () => {
    const dash = strides.map(v => v*g.unit);
    ctx.setLineDash(dash);
    ctx.beginPath();
    ctx.moveTo(g.x0,g.y0);
    ctx.lineTo(g.x1,g.y1);
    ctx.stroke();
    ctx.setLineDash([]);
  });
}

function drawRouteParallel(g, color, width, offset){
  withRouteStroke(color, width, 1, () => {
    for (const side of [-offset, offset]){
      const a = routePoint(g, 0, side);
      const b = routePoint(g, g.total, side);
      ctx.beginPath();
      ctx.moveTo(a[0],a[1]);
      ctx.lineTo(b[0],b[1]);
      ctx.stroke();
    }
  });
}

function drawRouteSpine(g, color, width, alpha){
  drawRouteStrides(g, color, Math.max(1, width*0.7), [], alpha);
}

function drawRouteTicks(g, color, width, spacing, halfLen){
  withRouteStroke(color, width*0.75, 1, () => {
    for (let t=spacing*0.5; t<g.total; t+=spacing){
      const p = routePoint(g, t, 0);
      ctx.beginPath();
      ctx.moveTo(p[0]-g.nx*halfLen, p[1]-g.ny*halfLen);
      ctx.lineTo(p[0]+g.nx*halfLen, p[1]+g.ny*halfLen);
      ctx.stroke();
    }
  });
}

function drawRouteJagged(g, color, width, spacing, amp){
  withRouteStroke(color, width, 1, () => {
    let prev = [g.x0,g.y0], sign = 1;
    for (let t=spacing; t<g.total; t+=spacing){
      const p = routePoint(g, t, amp*sign);
      ctx.beginPath();
      ctx.moveTo(prev[0],prev[1]);
      ctx.lineTo(p[0],p[1]);
      ctx.stroke();
      prev = p;
      sign = -sign;
    }
    ctx.beginPath();
    ctx.moveTo(prev[0],prev[1]);
    ctx.lineTo(g.x1,g.y1);
    ctx.stroke();
  });
}

function drawRouteZigzag(g, color, width, spacing, amp){
  withRouteStroke(color, width, 1, () => {
    let prev = routePoint(g, 0, -amp), sign = 1;
    for (let t=spacing*0.5; t<g.total; t+=spacing){
      const p = routePoint(g, t, amp*sign);
      ctx.beginPath();
      ctx.moveTo(prev[0],prev[1]);
      ctx.lineTo(p[0],p[1]);
      ctx.stroke();
      prev = p;
      sign = -sign;
    }
    const end = routePoint(g, g.total, -amp*sign);
    ctx.beginPath();
    ctx.moveTo(prev[0],prev[1]);
    ctx.lineTo(end[0],end[1]);
    ctx.stroke();
  });
}

function drawRouteDots(g, color, width, spacing, hollow, alternating, alpha=1){
  withRouteStroke(color, width, alpha, () => {
    let i = 0;
    for (let t=spacing*0.5; t<g.total; t+=spacing){
      const p = routePoint(g, t, 0);
      const r = Math.max(1, width*(alternating && i%2===0 ? 0.85 : 0.55));
      ctx.beginPath();
      ctx.arc(p[0], p[1], r, 0, Math.PI*2);
      if (hollow) ctx.stroke(); else ctx.fill();
      i++;
    }
  });
}

function drawRouteDotClusters(g, color, width, spacing, count){
  withRouteStroke(color, width, 1, () => {
    const dotGap = g.unit*1.25;
    const r = Math.max(1, width*0.55);
    for (let t=spacing*0.5; t<g.total; t+=spacing){
      const center = (count-1)*0.5;
      for (let i=0; i<count; i++){
        const p = routePoint(g, t + (i-center)*dotGap, 0);
        ctx.beginPath();
        ctx.arc(p[0], p[1], r, 0, Math.PI*2);
        ctx.fill();
      }
    }
  });
}

function drawRouteDoubleTaps(g, color, width, spacing){
  const pairGap = g.unit*1.3;
  const half = width*1.8;
  withRouteStroke(color, width*0.8, 1, () => {
    for (let t=spacing*0.5; t<g.total; t+=spacing){
      for (const local of [-pairGap*0.5, pairGap*0.5]){
        const p = routePoint(g, t+local, 0);
        ctx.beginPath();
        ctx.moveTo(p[0]-g.nx*half, p[1]-g.ny*half);
        ctx.lineTo(p[0]+g.nx*half, p[1]+g.ny*half);
        ctx.stroke();
      }
    }
  });
}

function drawRouteChevrons(g, color, width, spacing){
  const size = g.unit*1.8;
  withRouteStroke(color, width, 1, () => {
    for (let t=spacing*0.5; t<g.total; t+=spacing){
      const tip = routePoint(g, t+size*0.35, 0);
      const back = routePoint(g, t-size*0.35, 0);
      ctx.beginPath();
      ctx.moveTo(back[0]+g.nx*size*0.35, back[1]+g.ny*size*0.35);
      ctx.lineTo(tip[0], tip[1]);
      ctx.moveTo(back[0]-g.nx*size*0.35, back[1]-g.ny*size*0.35);
      ctx.lineTo(tip[0], tip[1]);
      ctx.stroke();
    }
  });
}

function drawRouteTriangles(g, color, width, spacing){
  const size = Math.max(g.unit*1.7, width*2);
  withRouteStroke(color, width, 1, () => {
    for (let t=spacing*0.5; t<g.total; t+=spacing){
      const tip = routePoint(g, t+size*0.45, 0);
      const base = routePoint(g, t-size*0.35, 0);
      ctx.beginPath();
      ctx.moveTo(tip[0], tip[1]);
      ctx.lineTo(base[0]+g.nx*size*0.38, base[1]+g.ny*size*0.38);
      ctx.lineTo(base[0]-g.nx*size*0.38, base[1]-g.ny*size*0.38);
      ctx.closePath();
      ctx.fill();
    }
  });
}

function drawRouteBursts(g, color, width, spacing){
  const radius = Math.max(2, width*1.6);
  withRouteStroke(color, width*0.65, 1, () => {
    for (let t=spacing*0.5; t<g.total; t+=spacing){
      const p = routePoint(g, t, 0);
      const a = routePoint(g, t-radius, 0);
      const b = routePoint(g, t+radius, 0);
      ctx.beginPath();
      ctx.moveTo(a[0],a[1]);
      ctx.lineTo(b[0],b[1]);
      ctx.moveTo(p[0]-g.nx*radius, p[1]-g.ny*radius);
      ctx.lineTo(p[0]+g.nx*radius, p[1]+g.ny*radius);
      ctx.stroke();
      ctx.beginPath();
      ctx.arc(p[0], p[1], Math.max(1,width*0.45), 0, Math.PI*2);
      ctx.fill();
    }
  });
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
  const force = p.force_name || p.force_id || "";
  if (sub && force) return `${p.faction_id}: ${sub} / ${force}`;
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
