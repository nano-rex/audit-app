// QR codes for fixed assets: an encoder (ISO/IEC 18004, byte mode, versions 1-40) with no
// dependencies, and a dialog that shows an asset's code as a QR image the person can save to
// the device or share to other apps. The image is a PNG with the asset's name and code under the
// symbol, so a printed label can be read by eye as well as scanned.

// Error correction codewords per block and number of blocks, by level then version (index 0 unused).
const QR_ECC_CODEWORDS = {
  L: [-1, 7, 10, 15, 20, 26, 18, 20, 24, 30, 18, 20, 24, 26, 30, 22, 24, 28, 30, 28, 28, 28, 28, 30, 30, 26, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  M: [-1, 10, 16, 26, 18, 24, 16, 18, 22, 22, 26, 30, 22, 22, 24, 24, 28, 28, 26, 26, 26, 26, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28, 28],
  Q: [-1, 13, 22, 18, 26, 18, 24, 18, 22, 20, 24, 28, 26, 24, 20, 30, 24, 28, 28, 26, 30, 28, 30, 30, 30, 30, 28, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
  H: [-1, 17, 28, 22, 16, 22, 28, 26, 26, 24, 28, 24, 28, 22, 24, 24, 30, 28, 28, 26, 28, 30, 24, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30, 30],
};
const QR_ECC_BLOCKS = {
  L: [-1, 1, 1, 1, 1, 1, 2, 2, 2, 2, 4, 4, 4, 4, 4, 6, 6, 6, 6, 7, 8, 8, 9, 9, 10, 12, 12, 12, 13, 14, 15, 16, 17, 18, 19, 19, 20, 21, 22, 24, 25],
  M: [-1, 1, 1, 1, 2, 2, 4, 4, 4, 5, 5, 5, 8, 9, 9, 10, 10, 11, 13, 14, 16, 17, 17, 18, 20, 21, 23, 25, 26, 28, 29, 31, 33, 35, 37, 38, 40, 43, 45, 47, 49],
  Q: [-1, 1, 1, 2, 2, 4, 4, 6, 6, 8, 8, 8, 10, 12, 16, 12, 17, 16, 18, 21, 20, 23, 23, 25, 27, 29, 34, 34, 35, 38, 40, 43, 45, 48, 51, 53, 56, 59, 62, 65, 68],
  H: [-1, 1, 1, 2, 4, 4, 4, 5, 6, 8, 8, 11, 11, 16, 16, 18, 16, 19, 21, 25, 25, 25, 34, 30, 32, 35, 37, 40, 42, 45, 48, 51, 54, 57, 60, 63, 66, 70, 74, 77, 81],
};
const QR_FORMAT_LEVEL = { L: 1, M: 0, Q: 3, H: 2 };
const QR_QUIET_ZONE = 4;

function qrUtf8(text) {
  const bytes = [];
  for (const char of String(text)) {
    let code = char.codePointAt(0);
    if (code < 0x80) bytes.push(code);
    else if (code < 0x800) bytes.push(0xc0 | code >> 6, 0x80 | code & 63);
    else if (code < 0x10000) bytes.push(0xe0 | code >> 12, 0x80 | code >> 6 & 63, 0x80 | code & 63);
    else bytes.push(0xf0 | code >> 18, 0x80 | code >> 12 & 63, 0x80 | code >> 6 & 63, 0x80 | code & 63);
  }
  return bytes;
}

// Modules available for data and error correction, after the function patterns.
function qrRawModules(version) {
  let count = (16 * version + 128) * version + 64;
  if (version >= 2) {
    const align = Math.floor(version / 7) + 2;
    count -= (25 * align - 10) * align - 55;
    if (version >= 7) count -= 36;
  }
  return count;
}

function qrDataCodewords(version, level) {
  return Math.floor(qrRawModules(version) / 8) - QR_ECC_CODEWORDS[level][version] * QR_ECC_BLOCKS[level][version];
}

function qrAlignmentPositions(version) {
  if (version === 1) return [];
  const count = Math.floor(version / 7) + 2;
  const step = Math.floor((version * 8 + count * 3 + 5) / (count * 4 - 4)) * 2;
  const positions = [6];
  for (let position = version * 4 + 10; positions.length < count; position -= step) positions.splice(1, 0, position);
  return positions;
}

function qrMultiply(a, b) {
  let product = 0;
  for (let bit = 7; bit >= 0; bit--) {
    product = product << 1 ^ (product >>> 7) * 0x11d;
    product ^= (b >>> bit & 1) * a;
  }
  return product;
}

function qrReedSolomon(data, degree) {
  const divisor = new Array(degree).fill(0);
  divisor[degree - 1] = 1;
  let root = 1;
  for (let i = 0; i < degree; i++) {
    for (let j = 0; j < degree; j++) {
      divisor[j] = qrMultiply(divisor[j], root);
      if (j + 1 < degree) divisor[j] ^= divisor[j + 1];
    }
    root = qrMultiply(root, 2);
  }
  const remainder = new Array(degree).fill(0);
  for (const byte of data) {
    const factor = byte ^ remainder.shift();
    remainder.push(0);
    divisor.forEach((coefficient, i) => { remainder[i] ^= qrMultiply(coefficient, factor); });
  }
  return remainder;
}

// Split data into blocks, add each block's error correction, and interleave them.
function qrCodewords(data, version, level) {
  const blocks = QR_ECC_BLOCKS[level][version];
  const eccLength = QR_ECC_CODEWORDS[level][version];
  const raw = Math.floor(qrRawModules(version) / 8);
  const shortBlocks = blocks - raw % blocks;
  const shortLength = Math.floor(raw / blocks);
  const split = [];
  for (let i = 0, offset = 0; i < blocks; i++) {
    const length = shortLength - eccLength + (i < shortBlocks ? 0 : 1);
    const chunk = data.slice(offset, offset + length);
    offset += length;
    const ecc = qrReedSolomon(chunk, eccLength);
    if (i < shortBlocks) chunk.push(0);
    split.push(chunk.concat(ecc));
  }
  const result = [];
  for (let i = 0; i < split[0].length; i++) {
    split.forEach((block, j) => {
      if (i !== shortLength - eccLength || j >= shortBlocks) result.push(block[i]);
    });
  }
  return result;
}

const QR_MASKS = [
  (x, y) => (x + y) % 2 === 0,
  (x, y) => y % 2 === 0,
  (x) => x % 3 === 0,
  (x, y) => (x + y) % 3 === 0,
  (x, y) => (Math.floor(x / 3) + Math.floor(y / 2)) % 2 === 0,
  (x, y) => x * y % 2 + x * y % 3 === 0,
  (x, y) => (x * y % 2 + x * y % 3) % 2 === 0,
  (x, y) => ((x + y) % 2 + x * y % 3) % 2 === 0,
];

// The penalty score of ISO/IEC 18004 section 7.8.3; the mask with the lowest score is used.
function qrPenalty(modules) {
  const size = modules.length;
  let score = 0;
  let dark = 0;
  const lines = [];
  for (let i = 0; i < size; i++) {
    lines.push(modules[i]);
    lines.push(modules.map((row) => row[i]));
  }
  const finderA = [1, 0, 1, 1, 1, 0, 1, 0, 0, 0, 0];
  const finderB = [0, 0, 0, 0, 1, 0, 1, 1, 1, 0, 1];
  for (const line of lines) {
    for (let i = 0, run = 1; i < size; i++) {
      if (i + 1 < size && line[i + 1] === line[i]) { run++; continue; }
      if (run >= 5) score += run - 2;
      run = 1;
    }
    for (let i = 0; i + 11 <= size; i++) {
      if (finderA.every((bit, k) => line[i + k] === Boolean(bit))) score += 40;
      if (finderB.every((bit, k) => line[i + k] === Boolean(bit))) score += 40;
    }
  }
  for (let y = 0; y < size; y++) {
    for (let x = 0; x < size; x++) {
      if (modules[y][x]) dark++;
      if (x + 1 < size && y + 1 < size) {
        const color = modules[y][x];
        if (modules[y][x + 1] === color && modules[y + 1][x] === color && modules[y + 1][x + 1] === color) score += 3;
      }
    }
  }
  const total = size * size;
  score += (Math.ceil(Math.abs(dark * 20 - total * 10) / total) - 1) * 10;
  return score;
}

// Encode text as a QR symbol. Returns a square array of rows, true for a dark module.
// The smallest version that fits is used, at the highest error correction that still fits it.
function qrEncode(text, minimumLevel = "M") {
  const bytes = qrUtf8(text);
  const levels = ["L", "M", "Q", "H"];
  let level = minimumLevel;
  let version = 1;
  const capacity = (v, l) => qrDataCodewords(v, l) * 8;
  const needed = (v) => 4 + (v <= 9 ? 8 : 16) + bytes.length * 8;
  while (version <= 40 && needed(version) > capacity(version, level)) version++;
  if (version > 40) throw new Error("This text is too long for a QR code.");
  for (const higher of levels.slice(levels.indexOf(level) + 1)) {
    if (needed(version) <= capacity(version, higher)) level = higher;
  }

  const bits = [];
  const append = (value, length) => { for (let i = length - 1; i >= 0; i--) bits.push(value >>> i & 1); };
  append(4, 4);
  append(bytes.length, version <= 9 ? 8 : 16);
  bytes.forEach((byte) => append(byte, 8));
  const limit = capacity(version, level);
  append(0, Math.min(4, limit - bits.length));
  append(0, (8 - bits.length % 8) % 8);
  for (let pad = 0xec; bits.length < limit; pad ^= 0xec ^ 0x11) append(pad, 8);
  const data = [];
  for (let i = 0; i < bits.length; i += 8) data.push(bits.slice(i, i + 8).reduce((byte, bit) => byte << 1 | bit, 0));
  const codewords = qrCodewords(data, version, level);

  const size = version * 4 + 17;
  const modules = Array.from({ length: size }, () => new Array(size).fill(false));
  const reserved = Array.from({ length: size }, () => new Array(size).fill(false));
  const set = (x, y, dark) => { modules[y][x] = dark; reserved[y][x] = true; };

  for (let i = 0; i < size; i++) { set(6, i, i % 2 === 0); set(i, 6, i % 2 === 0); }
  for (const [cx, cy] of [[3, 3], [size - 4, 3], [3, size - 4]]) {
    for (let dy = -4; dy <= 4; dy++) {
      for (let dx = -4; dx <= 4; dx++) {
        const x = cx + dx;
        const y = cy + dy;
        const distance = Math.max(Math.abs(dx), Math.abs(dy));
        if (x >= 0 && x < size && y >= 0 && y < size) set(x, y, distance !== 2 && distance !== 4);
      }
    }
  }
  const align = qrAlignmentPositions(version);
  align.forEach((ax, i) => align.forEach((ay, j) => {
    const corner = (i === 0 && j === 0) || (i === 0 && j === align.length - 1) || (i === align.length - 1 && j === 0);
    if (corner) return;
    for (let dy = -2; dy <= 2; dy++) for (let dx = -2; dx <= 2; dx++) set(ax + dx, ay + dy, Math.max(Math.abs(dx), Math.abs(dy)) !== 1);
  }));
  const drawFormat = (mask) => {
    const value = QR_FORMAT_LEVEL[level] << 3 | mask;
    let remainder = value;
    for (let i = 0; i < 10; i++) remainder = remainder << 1 ^ (remainder >>> 9) * 0x537;
    const format = (value << 10 | remainder) ^ 0x5412;
    const bit = (i) => (format >>> i & 1) === 1;
    for (let i = 0; i <= 5; i++) set(8, i, bit(i));
    set(8, 7, bit(6));
    set(8, 8, bit(7));
    set(7, 8, bit(8));
    for (let i = 9; i < 15; i++) set(14 - i, 8, bit(i));
    for (let i = 0; i < 8; i++) set(size - 1 - i, 8, bit(i));
    for (let i = 8; i < 15; i++) set(8, size - 15 + i, bit(i));
    set(8, size - 8, true);
  };
  drawFormat(0);
  if (version >= 7) {
    let remainder = version;
    for (let i = 0; i < 12; i++) remainder = remainder << 1 ^ (remainder >>> 11) * 0x1f25;
    const value = version << 12 | remainder;
    for (let i = 0; i < 18; i++) {
      const dark = (value >>> i & 1) === 1;
      const a = size - 11 + i % 3;
      const b = Math.floor(i / 3);
      set(a, b, dark);
      set(b, a, dark);
    }
  }

  let index = 0;
  for (let right = size - 1; right >= 1; right -= 2) {
    if (right === 6) right = 5;
    for (let vertical = 0; vertical < size; vertical++) {
      for (let j = 0; j < 2; j++) {
        const x = right - j;
        const upward = (right + 1 & 2) === 0;
        const y = upward ? size - 1 - vertical : vertical;
        if (!reserved[y][x] && index < codewords.length * 8) {
          modules[y][x] = (codewords[index >>> 3] >>> 7 - (index & 7) & 1) === 1;
          index++;
        }
      }
    }
  }

  let best = null;
  for (let mask = 0; mask < 8; mask++) {
    const candidate = modules.map((row, y) => row.map((dark, x) => (!reserved[y][x] && QR_MASKS[mask](x, y) ? !dark : dark)));
    const saved = modules.map((row) => row.slice());
    modules.splice(0, size, ...candidate);
    drawFormat(mask);
    const score = qrPenalty(modules);
    if (!best || score < best.score) best = { score, modules: modules.map((row) => row.slice()) };
    modules.splice(0, size, ...saved);
  }
  return best.modules;
}

// An SVG of the symbol, for showing it on screen at any size.
function qrSvg(modules, label = "QR code") {
  const size = modules.length + QR_QUIET_ZONE * 2;
  let path = "";
  modules.forEach((row, y) => row.forEach((dark, x) => { if (dark) path += `M${x + QR_QUIET_ZONE} ${y + QR_QUIET_ZONE}h1v1h-1z`; }));
  return `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 ${size} ${size}" shape-rendering="crispEdges" role="img" aria-label="${escapeAttr(label)}"><rect width="${size}" height="${size}" fill="#fff"/><path d="${path}" fill="#000"/></svg>`;
}

// Wrap text to the canvas width, at most two lines, ending with an ellipsis when it is cut.
function qrCaptionLines(context, text, width) {
  const words = String(text || "").split(/\s+/).filter(Boolean);
  const lines = [];
  let line = "";
  for (const word of words) {
    const next = line ? `${line} ${word}` : word;
    if (context.measureText(next).width <= width || !line) { line = next; continue; }
    lines.push(line);
    line = word;
  }
  if (line) lines.push(line);
  if (lines.length > 2) {
    let last = lines[1];
    while (last && context.measureText(`${last}…`).width > width) last = last.slice(0, -1);
    return [lines[0], `${last}…`];
  }
  return lines;
}

// A PNG of the symbol with the asset's name and code underneath, about 30 mm wide at 300 dpi.
function qrPng(modules, { title = "", code = "" } = {}) {
  const scale = 12;
  const side = (modules.length + QR_QUIET_ZONE * 2) * scale;
  const width = Math.max(side, 480);
  const canvas = document.createElement("canvas");
  const context = canvas.getContext("2d");
  const font = (weight, px) => `${weight} ${px}px "Noto Sans SC", "Noto Sans", Arial, sans-serif`;
  context.font = font(700, 30);
  const titleLines = qrCaptionLines(context, title, width - 48);
  const captionHeight = (titleLines.length ? titleLines.length * 38 : 0) + (code ? 40 : 0) + (titleLines.length || code ? 24 : 0);
  canvas.width = width;
  canvas.height = side + captionHeight;
  context.fillStyle = "#fff";
  context.fillRect(0, 0, canvas.width, canvas.height);
  context.fillStyle = "#000";
  const left = Math.floor((width - side) / 2) + QR_QUIET_ZONE * scale;
  modules.forEach((row, y) => row.forEach((dark, x) => { if (dark) context.fillRect(left + x * scale, QR_QUIET_ZONE * scale + y * scale, scale, scale); }));
  context.textAlign = "center";
  context.textBaseline = "top";
  let top = side - QR_QUIET_ZONE * scale / 2;
  context.font = font(700, 30);
  titleLines.forEach((line) => { context.fillText(line, width / 2, top); top += 38; });
  if (code) {
    context.font = font(400, 28);
    context.fillText(code, width / 2, top);
  }
  return new Promise((resolve, reject) => canvas.toBlob((blob) => (blob ? resolve(blob) : reject(new Error("The QR image could not be created."))), "image/png"));
}

function qrFileName(code) {
  const safe = String(code || "asset").replace(/[^A-Za-z0-9._-]+/g, "-").replace(/^-+|-+$/g, "") || "asset";
  return `QR-${safe}.png`;
}

// The value a fixed asset's QR code carries: its QR code field, which is filled from its Code.
function assetQrValue(row) {
  return String(row?.qr_code || row?.qrCode || row?.code || row?.asset_id || "").trim();
}

const qrDialogState = { file: null, url: "" };

async function openAssetQr(asset) {
  const dialog = document.getElementById("asset-qr-dialog");
  const value = assetQrValue(asset);
  if (!dialog || !value) return;
  const title = asset.name || "Fixed Asset";
  const modules = qrEncode(value);
  dialog.querySelector("[data-qr-title]").textContent = title;
  dialog.querySelector("[data-qr-image]").innerHTML = qrSvg(modules, `QR code for ${value}`);
  dialog.querySelector("[data-qr-value]").textContent = value;
  dialog.querySelector("[data-qr-detail]").textContent = [asset.outlet, asset.location || asset.zone].filter(Boolean).join(" | ");
  dialog.querySelector("[data-qr-status]").textContent = "";
  if (qrDialogState.url) URL.revokeObjectURL(qrDialogState.url);
  qrDialogState.file = null;
  qrDialogState.url = "";
  const share = dialog.querySelector("[data-qr-share]");
  share.hidden = typeof navigator === "undefined" || typeof navigator.share !== "function";
  if (!dialog.open) dialog.showModal();
  const blob = await qrPng(modules, { title, code: value });
  const name = qrFileName(value);
  qrDialogState.file = typeof File === "function" ? new File([blob], name, { type: "image/png" }) : blob;
  qrDialogState.file.fileName = name;
  qrDialogState.url = URL.createObjectURL(blob);
  qrDialogState.title = title;
  qrDialogState.value = value;
  share.hidden = !(navigator.canShare?.({ files: [qrDialogState.file] }) ?? typeof navigator.share === "function");
}

function saveAssetQr() {
  if (!qrDialogState.url) return;
  const link = document.createElement("a");
  link.href = qrDialogState.url;
  link.download = qrDialogState.file.fileName || qrDialogState.file.name;
  document.body.append(link);
  link.click();
  link.remove();
  setText("[data-qr-status]", `Saved ${link.download}.`);
}

async function shareAssetQr() {
  const { file, title, value } = qrDialogState;
  if (!file) return;
  const data = { files: [file], title: `QR code: ${title}`, text: `${title} (${value})` };
  try {
    if (navigator.canShare && !navigator.canShare(data)) {
      saveAssetQr();
      setText("[data-qr-status]", "This device cannot share images from the browser, so the QR image was saved instead.");
      return;
    }
    await navigator.share(data);
    setText("[data-qr-status]", "");
  } catch (error) {
    if (error?.name !== "AbortError") setText("[data-qr-status]", "The QR image could not be shared. Save it instead, then share it from your files or photos.");
  }
}

if (typeof document !== "undefined" && document.getElementById?.("asset-qr-dialog")) {
  document.addEventListener("click", (event) => {
    const open = event.target.closest("[data-asset-qr]");
    if (open) {
      event.preventDefault();
      openAssetQr(JSON.parse(open.dataset.assetQr)).catch((error) => setText("[data-qr-status]", error.message));
      return;
    }
    if (event.target.closest("[data-qr-save]")) saveAssetQr();
    else if (event.target.closest("[data-qr-share]")) shareAssetQr();
  });
}
