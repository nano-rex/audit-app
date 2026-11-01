// Photo viewer: opens any stored photo in a dialog with zoom (wheel, pinch, buttons, double-click)
// and panning (drag). A set of photos can be stepped through; a marked photo can be compared
// with its original.
const photoViewer = { images: [], index: 0, marked: true, fit: 1, scale: 1, x: 0, y: 0, pointers: new Map(), pinch: null };
const PHOTO_MAX_ZOOM = 8;

function photoThumbnail(image) {
  const source = imageSource(image, Boolean(imageSource(image, true)));
  return source.startsWith("/api/media/") ? `${source}?thumb=1` : source;
}

// Markup for one clickable thumbnail. Thumbnails in the same container form a set.
function photoThumbnailButton(image, label = "View photo") {
  if (!imageSource(image)) return "";
  const data = { url: imageSource(image), markedUrl: imageSource(image, true), name: imageLabel(image), uploadedAt: image.uploadedAt || "" };
  return `<button type="button" class="photo-thumb" data-view-photo='${escapeAttr(JSON.stringify(data))}' aria-label="${escapeAttr(label)}: ${escapeAttr(imageLabel(image))}"><img src="${escapeAttr(photoThumbnail(image))}" alt="" loading="lazy"></button>`;
}

// Markup for a button that opens a whole set, used on list rows.
function photoSetButton(images, label) {
  const set = (images || []).filter((image) => imageSource(image)).map((image) => ({ url: imageSource(image), markedUrl: imageSource(image, true), name: imageLabel(image), uploadedAt: image.uploadedAt || "", caption: image.caption || "" }));
  if (!set.length) return "";
  return `<button type="button" class="outline" data-view-photos='${escapeAttr(JSON.stringify(set))}'>${escapeHtml(label)} (${set.length})</button>`;
}

function openPhotoViewer(images, index = 0) {
  const dialog = document.getElementById("photo-viewer");
  if (!dialog || !images.length) return;
  photoViewer.images = images;
  if (!dialog.open) dialog.showModal();
  showPhoto(index);
  dialog.querySelector("[data-photo-stage]").focus();
}

function showPhoto(index) {
  const dialog = document.getElementById("photo-viewer");
  const image = dialog.querySelector("[data-photo-image]");
  photoViewer.index = (index + photoViewer.images.length) % photoViewer.images.length;
  const photo = photoViewer.images[photoViewer.index];
  photoViewer.marked = Boolean(photo.markedUrl);
  const many = photoViewer.images.length > 1;
  dialog.querySelectorAll("[data-photo-step]").forEach((button) => { button.hidden = !many; });
  image.hidden = true;  // Until it has loaded, so the previous photo is not shown at the new size.
  image.onload = () => { image.hidden = false; fitPhoto(); };
  image.onerror = () => { setText("[data-photo-caption]", "This photo could not be loaded."); };
  renderPhotoSource();
}

function renderPhotoSource() {
  const dialog = document.getElementById("photo-viewer");
  const photo = photoViewer.images[photoViewer.index];
  const image = dialog.querySelector("[data-photo-image]");
  image.src = photoViewer.marked && photo.markedUrl ? photo.markedUrl : photo.url;
  image.alt = photo.name || "Photo";
  const version = dialog.querySelector("[data-photo-version]");
  version.hidden = !photo.markedUrl;
  version.textContent = photoViewer.marked ? "Show original" : "Show marked";
  setText("[data-photo-title]", photo.name || "Photo");
  const taken = photo.uploadedAt ? new Date(photo.uploadedAt).toLocaleString() : "";
  const position = photoViewer.images.length > 1 ? `${photoViewer.index + 1} of ${photoViewer.images.length}` : "";
  setText("[data-photo-caption]", [photo.caption, position, taken, photo.markedUrl ? (photoViewer.marked ? "Marked" : "Original") : ""].filter(Boolean).join(" · "));
}

function photoStageSize() {
  const stage = document.querySelector("[data-photo-stage]");
  return { width: stage.clientWidth, height: stage.clientHeight };
}

// The scale at which the whole photo is visible; zoom is measured against it.
function fitPhoto() {
  const image = document.querySelector("[data-photo-image]");
  const stage = photoStageSize();
  if (!image.naturalWidth || !stage.width) return;
  photoViewer.fit = Math.min(stage.width / image.naturalWidth, stage.height / image.naturalHeight);
  photoViewer.scale = photoViewer.fit;
  applyPhotoTransform();
}

// Keep the photo over the stage: centred when it is smaller, never dragged out of view when larger.
function clampPhotoOffset(value, stage, size) {
  return size <= stage ? (stage - size) / 2 : Math.min(0, Math.max(stage - size, value));
}

function applyPhotoTransform() {
  const image = document.querySelector("[data-photo-image]");
  const stage = photoStageSize();
  photoViewer.x = clampPhotoOffset(photoViewer.x, stage.width, image.naturalWidth * photoViewer.scale);
  photoViewer.y = clampPhotoOffset(photoViewer.y, stage.height, image.naturalHeight * photoViewer.scale);
  image.style.transform = `translate(${photoViewer.x}px, ${photoViewer.y}px) scale(${photoViewer.scale})`;
  const zoom = Math.round(photoViewer.scale / photoViewer.fit * 100);
  setText("[data-photo-zoom-level]", `${zoom}%`);
  document.querySelector("[data-photo-stage]").classList.toggle("zoomed", zoom > 100);
}

// Zoom by a factor, keeping the point under (originX, originY) in place.
function zoomPhoto(factor, originX, originY) {
  const stage = photoStageSize();
  const x = originX ?? stage.width / 2;
  const y = originY ?? stage.height / 2;
  const scale = Math.min(photoViewer.fit * PHOTO_MAX_ZOOM, Math.max(photoViewer.fit, photoViewer.scale * factor));
  const ratio = scale / photoViewer.scale;
  photoViewer.x = x - (x - photoViewer.x) * ratio;
  photoViewer.y = y - (y - photoViewer.y) * ratio;
  photoViewer.scale = scale;
  applyPhotoTransform();
}

function photoStagePoint(event) {
  const bounds = document.querySelector("[data-photo-stage]").getBoundingClientRect();
  return { x: event.clientX - bounds.left, y: event.clientY - bounds.top };
}

if (typeof document !== "undefined" && document.getElementById?.("photo-viewer")) {
  const dialog = document.getElementById("photo-viewer");
  const stage = dialog.querySelector("[data-photo-stage]");

  document.addEventListener("click", (event) => {
    const single = event.target.closest("[data-view-photo]");
    if (single) {
      event.preventDefault();
      // Thumbnails shown together are browsed together.
      const group = single.closest("[data-saved-images], .saved-images, [data-photo-group]") || single.parentElement;
      const buttons = [...group.querySelectorAll("[data-view-photo]")];
      openPhotoViewer(buttons.map((button) => JSON.parse(button.dataset.viewPhoto)), buttons.indexOf(single));
      return;
    }
    const set = event.target.closest("[data-view-photos]");
    if (set) {
      event.preventDefault();
      openPhotoViewer(JSON.parse(set.dataset.viewPhotos), 0);
    }
  });

  dialog.addEventListener("click", (event) => {
    const zoom = event.target.closest("[data-photo-zoom]")?.dataset.photoZoom;
    if (zoom === "in") zoomPhoto(1.5);
    if (zoom === "out") zoomPhoto(1 / 1.5);
    if (zoom === "fit") fitPhoto();
    const step = event.target.closest("[data-photo-step]");
    if (step) showPhoto(photoViewer.index + Number(step.dataset.photoStep));
    if (event.target.closest("[data-photo-version]")) {
      photoViewer.marked = !photoViewer.marked;
      renderPhotoSource();
    }
  });

  stage.addEventListener("wheel", (event) => {
    event.preventDefault();
    const point = photoStagePoint(event);
    zoomPhoto(event.deltaY < 0 ? 1.2 : 1 / 1.2, point.x, point.y);
  }, { passive: false });

  stage.addEventListener("dblclick", (event) => {
    const point = photoStagePoint(event);
    if (photoViewer.scale > photoViewer.fit * 1.01) fitPhoto();
    else zoomPhoto(2.5, point.x, point.y);
  });

  stage.addEventListener("pointerdown", (event) => {
    stage.setPointerCapture?.(event.pointerId);
    photoViewer.pointers.set(event.pointerId, photoStagePoint(event));
    photoViewer.pinch = null;
  });

  stage.addEventListener("pointermove", (event) => {
    const previous = photoViewer.pointers.get(event.pointerId);
    if (!previous) return;
    const point = photoStagePoint(event);
    photoViewer.pointers.set(event.pointerId, point);
    const points = [...photoViewer.pointers.values()];
    if (points.length === 2) {
      // Two fingers: zoom about their midpoint by the change in their distance.
      const distance = Math.hypot(points[0].x - points[1].x, points[0].y - points[1].y);
      if (photoViewer.pinch) zoomPhoto(distance / photoViewer.pinch, (points[0].x + points[1].x) / 2, (points[0].y + points[1].y) / 2);
      photoViewer.pinch = distance;
      return;
    }
    photoViewer.x += point.x - previous.x;
    photoViewer.y += point.y - previous.y;
    applyPhotoTransform();
  });

  ["pointerup", "pointercancel", "pointerleave"].forEach((type) => stage.addEventListener(type, (event) => {
    photoViewer.pointers.delete(event.pointerId);
    photoViewer.pinch = null;
  }));

  dialog.addEventListener("keydown", (event) => {
    if (event.key === "+" || event.key === "=") zoomPhoto(1.5);
    else if (event.key === "-") zoomPhoto(1 / 1.5);
    else if (event.key === "0") fitPhoto();
    else if (event.key === "ArrowRight" && photoViewer.images.length > 1) showPhoto(photoViewer.index + 1);
    else if (event.key === "ArrowLeft" && photoViewer.images.length > 1) showPhoto(photoViewer.index - 1);
    else return;
    event.preventDefault();
  });

  dialog.addEventListener("close", () => { dialog.querySelector("[data-photo-image]").removeAttribute("src"); });
  window.addEventListener("resize", () => { if (dialog.open) fitPhoto(); });
}
