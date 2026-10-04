// A canvas to draw a signature on with a finger, pen, or mouse. Used for signing an
// inspection and for the signature kept on an account.
function setupSignaturePad(canvas) {
  if (!canvas) return null;
  const ctx = canvas.getContext("2d");
  let drawing = false;
  let last = null;
  let marked = false;
  const point = (event) => {
    const rect = canvas.getBoundingClientRect();
    return { x: (event.clientX - rect.left) * (canvas.width / rect.width), y: (event.clientY - rect.top) * (canvas.height / rect.height) };
  };
  const pad = {
    clear() {
      ctx.fillStyle = "#fff";
      ctx.fillRect(0, 0, canvas.width, canvas.height);
      marked = false;
    },
    // Show an existing signature, scaled to fit, so it can be kept or drawn over.
    show(source) {
      pad.clear();
      if (!source) return;
      const image = new Image();
      image.addEventListener("load", () => {
        const ratio = Math.min(canvas.width / image.width, canvas.height / image.height);
        const width = image.width * ratio;
        const height = image.height * ratio;
        ctx.drawImage(image, (canvas.width - width) / 2, (canvas.height - height) / 2, width, height);
        marked = true;
      });
      image.src = source;
    },
    isBlank: () => !marked,
    toDataUrl: () => canvas.toDataURL("image/png"),
  };
  canvas.addEventListener("pointerdown", (event) => {
    drawing = true;
    last = point(event);
    canvas.setPointerCapture?.(event.pointerId);
  });
  canvas.addEventListener("pointermove", (event) => {
    if (!drawing) return;
    const next = point(event);
    ctx.strokeStyle = "#111d27";
    ctx.lineWidth = 3;
    ctx.lineCap = "round";
    ctx.beginPath();
    ctx.moveTo(last.x, last.y);
    ctx.lineTo(next.x, next.y);
    ctx.stroke();
    last = next;
    marked = true;
  });
  ["pointerup", "pointercancel", "pointerleave"].forEach((type) => canvas.addEventListener(type, () => { drawing = false; }));
  pad.clear();
  return pad;
}
