const listPages = new Map();

function paginateList(key, rows, filters, render) {
  const filterKey = JSON.stringify(filters);
  const state = listPages.get(key) || { page: 1, size: 25 };
  if (state.filterKey !== filterKey) state.page = 1;
  state.filterKey = filterKey;
  state.render = render;
  state.pages = Math.max(1, Math.ceil(rows.length / state.size));
  state.page = Math.max(1, Math.min(state.page, state.pages));
  listPages.set(key, state);
  const start = (state.page - 1) * state.size;
  const items = rows.slice(start, start + state.size);
  const button = (label, page, disabled) => `<button class="outline" type="button" data-list-page="${key}" data-page="${page}" ${disabled ? "disabled" : ""}>${label}</button>`;
  const controls = `<nav class="pager list-pager" aria-label="${key} pages">
    <span aria-live="polite">Showing ${rows.length ? start + 1 : 0}–${start + items.length} of ${rows.length}</span>
    <label>Per page <select data-list-size="${key}">${[10, 25, 50, 100].map((size) => `<option ${size === state.size ? "selected" : ""}>${size}</option>`).join("")}</select></label>
    ${button("First", 1, state.page === 1)}${button("Previous", state.page - 1, state.page === 1)}
    <label>Page <input type="number" min="1" max="${state.pages}" value="${state.page}" data-list-jump="${key}" aria-label="${key} page number"> of ${state.pages}</label>
    ${button("Next", state.page + 1, state.page === state.pages)}${button("Last", state.pages, state.page === state.pages)}
  </nav>`;
  return { items, controls };
}

document.addEventListener("click", (event) => {
  const button = event.target.closest("[data-list-page]");
  if (!button || button.disabled) return;
  const state = listPages.get(button.dataset.listPage);
  if (!state) return;
  state.page = Number(button.dataset.page);
  state.render();
});

document.addEventListener("change", (event) => {
  const input = event.target;
  const key = input.dataset.listSize || input.dataset.listJump;
  const state = listPages.get(key);
  if (!state) return;
  const value = Number(input.value);
  if (input.dataset.listSize && [10, 25, 50, 100].includes(value)) {
    state.size = value;
    state.page = 1;
  } else if (input.dataset.listJump) {
    state.page = Number.isFinite(value) ? Math.max(1, Math.min(Math.floor(value), state.pages)) : 1;
  }
  state.render();
});
