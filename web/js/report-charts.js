// Charts for the Reports page, drawn as SVG and HTML in the app's colours (they follow the theme,
// light or dark). Every chart keeps its numbers visible, in a legend or beside each bar.
const CHART = {
  pass: "var(--success)", fail: "var(--red)", warn: "var(--orange)", info: "var(--blue)",
  accent: "var(--accent)", muted: "var(--border-strong)",
};
let reportScoring = { passMark: 70, excellentBand: 90, goodBand: 70, belowBand: 60 };

function scoreColour(score) {
  if (score >= reportScoring.goodBand) return CHART.pass;
  if (score >= reportScoring.belowBand) return CHART.warn;
  return CHART.fail;
}

function chartCard(title, note, body, wide = false) {
  return `<article class="panel chart-card${wide ? " chart-wide" : ""}"><h2>${escapeHtml(title)}</h2>${note ? `<p class="muted">${escapeHtml(note)}</p>` : ""}${body}</article>`;
}

function noChartData(text = "No data yet") {
  return `<p class="muted chart-empty">${escapeHtml(text)}</p>`;
}

// A ring of parts with a figure in its middle, and a legend with each part's count and share.
function donutChart(parts, centre, caption, footer = "") {
  const total = parts.reduce((sum, part) => sum + Math.max(0, part.value), 0);
  const radius = 42, circumference = 2 * Math.PI * radius;
  let offset = 0;
  const arcs = total ? parts.filter((part) => part.value > 0).map((part) => {
    const length = circumference * part.value / total;
    const arc = `<circle cx="60" cy="60" r="${radius}" style="stroke:${part.colour}" stroke-dasharray="${length.toFixed(2)} ${(circumference - length).toFixed(2)}" stroke-dashoffset="${(-offset).toFixed(2)}"></circle>`;
    offset += length;
    return arc;
  }).join("") : "";
  const label = parts.map((part) => `${part.label}: ${part.value}`).join(", ");
  return `<div class="donut-chart">
    <svg viewBox="0 0 120 120" role="img" aria-label="${escapeAttr(label)}">
      <circle class="donut-track" cx="60" cy="60" r="${radius}"></circle>
      <g transform="rotate(-90 60 60)">${arcs}</g>
      <text x="60" y="61" class="donut-value">${escapeHtml(String(centre))}</text>
      <text x="60" y="77" class="donut-caption">${escapeHtml(caption)}</text>
    </svg>
    <ul class="chart-legend">${parts.map((part) => `<li><i style="background:${part.colour}"></i><span>${escapeHtml(part.label)}</span>
      <b>${part.value}</b><small>${total ? Math.round(part.value * 100 / total) : 0}%</small></li>`).join("")}</ul>
  </div>${footer ? `<p class="chart-footer">${footer}</p>` : ""}`;
}

// Horizontal bars, each with its value; ghost draws a faint longer bar behind (e.g. the longest time).
function barList(rows, { max = null, suffix = "", mark = null, format = (value) => `${value}${suffix}` } = {}) {
  if (!rows.length) return noChartData();
  const top = max ?? Math.max(1, ...rows.map((row) => Math.max(row.value, row.ghost || 0)));
  return `<ol class="bar-list">${rows.map((row) => `<li>
      <div class="bar-list-label"><span title="${escapeAttr(row.label)}">${escapeHtml(row.label)}</span><strong>${escapeHtml(format(row.value))}</strong></div>
      <div class="bar-list-track" aria-hidden="true">
        ${row.ghost ? `<i class="bar-ghost" style="width:${Math.min(100, row.ghost * 100 / top)}%"></i>` : ""}
        <span style="width:${Math.min(100, row.value * 100 / top)}%;background:${row.colour || CHART.accent}"></span>
        ${mark != null ? `<em class="bar-mark" style="left:${Math.min(100, mark * 100 / top)}%"></em>` : ""}
      </div>${row.note ? `<small>${escapeHtml(row.note)}</small>` : ""}
    </li>`).join("")}</ol>${mark != null ? `<p class="chart-footer"><em class="bar-mark-key"></em> Pass mark ${mark}</p>` : ""}`;
}

// Columns over time (e.g. audits a month), with an optional line on a 0–100 scale (e.g. the score).
function columnLineChart(points, { barLabel, lineLabel = "", mark = null, stacks = null } = {}) {
  if (!points.length) return noChartData();
  const width = 560, height = 210, left = 34, right = lineLabel ? 34 : 10, top = 14, bottom = 32;
  const plotWidth = width - left - right, plotHeight = height - top - bottom;
  const totals = points.map((point) => stacks ? stacks.reduce((sum, stack) => sum + (point[stack.key] || 0), 0) : point.bar);
  const maxBar = Math.max(1, ...totals);
  const step = plotWidth / points.length;
  const barWidth = Math.min(46, step * 0.6);
  const y = (value) => top + plotHeight - value * plotHeight / maxBar;
  const yLine = (value) => top + plotHeight - value * plotHeight / 100;
  const ticks = [0, Math.ceil(maxBar / 2), maxBar];
  const grid = ticks.map((tick) => `<line class="chart-grid-line" x1="${left}" x2="${width - right}" y1="${y(tick)}" y2="${y(tick)}"></line>
    <text class="chart-axis" x="${left - 6}" y="${y(tick) + 4}" text-anchor="end">${tick}</text>`).join("");
  const bars = points.map((point, index) => {
    const x = left + step * index + (step - barWidth) / 2;
    let base = top + plotHeight;
    const pieces = (stacks || [{ key: "bar", colour: CHART.accent }]).map((stack) => {
      const value = point[stack.key] || 0;
      const h = value * plotHeight / maxBar;
      base -= h;
      return value ? `<rect x="${x.toFixed(1)}" y="${base.toFixed(1)}" width="${barWidth.toFixed(1)}" height="${h.toFixed(1)}" rx="3" style="fill:${stack.colour}"><title>${escapeHtml(point.label)} · ${escapeHtml(stack.label || barLabel)}: ${value}</title></rect>` : "";
    }).join("");
    return `${pieces}<text class="chart-value" x="${(x + barWidth / 2).toFixed(1)}" y="${(y(totals[index]) - 4).toFixed(1)}" text-anchor="middle">${totals[index]}</text>
      <text class="chart-axis" x="${(x + barWidth / 2).toFixed(1)}" y="${height - 12}" text-anchor="middle">${escapeHtml(point.label)}</text>`;
  }).join("");
  let line = "";
  if (lineLabel) {
    const coords = points.map((point, index) => [left + step * index + step / 2, yLine(point.line || 0)]);
    line = `<polyline class="chart-line" points="${coords.map(([cx, cy]) => `${cx.toFixed(1)},${cy.toFixed(1)}`).join(" ")}"></polyline>
      ${coords.map(([cx, cy], index) => `<circle class="chart-dot" cx="${cx.toFixed(1)}" cy="${cy.toFixed(1)}" r="4" style="fill:${scoreColour(points[index].line || 0)}"><title>${escapeHtml(points[index].label)} · ${escapeHtml(lineLabel)}: ${points[index].line}</title></circle>
        <text class="chart-line-value" x="${cx.toFixed(1)}" y="${(cy - 8).toFixed(1)}" text-anchor="middle">${points[index].line}</text>`).join("")}
      ${[0, 50, 100].map((tick) => `<text class="chart-axis" x="${width - right + 6}" y="${yLine(tick) + 4}">${tick}</text>`).join("")}
      ${mark != null ? `<line class="chart-mark" x1="${left}" x2="${width - right}" y1="${yLine(mark)}" y2="${yLine(mark)}"></line>` : ""}`;
  }
  const legend = [
    ...(stacks || [{ label: barLabel, colour: CHART.accent }]).map((stack) => `<li><i style="background:${stack.colour}"></i>${escapeHtml(stack.label)}</li>`),
    ...(lineLabel ? [`<li><i class="legend-line"></i>${escapeHtml(lineLabel)} (0–100)</li>`] : []),
    ...(lineLabel && mark != null ? [`<li><i class="legend-mark"></i>Pass mark ${mark}</li>`] : []),
  ].join("");
  return `<div class="chart-scroll"><svg class="column-chart" viewBox="0 0 ${width} ${height}" role="img" aria-label="${escapeAttr(barLabel)}">${grid}${bars}${line}</svg></div>
    <ul class="chart-legend chart-legend-row">${legend}</ul>`;
}

// Pairs of bars per group, e.g. each outlet's previous and current audit score.
function pairedBars(groups, names, colours, { max = 100, suffix = "" } = {}) {
  if (!groups.length) return noChartData();
  return `<ol class="bar-list paired">${groups.map((group) => `<li>
      <div class="bar-list-label"><span>${escapeHtml(group.label)}</span></div>
      ${names.map((name, index) => group.values[index] == null ? "" : `<div class="paired-row"><small>${escapeHtml(name)}</small>
        <div class="bar-list-track"><span style="width:${Math.min(100, group.values[index] * 100 / max)}%;background:${colours[index]}"></span></div>
        <strong>${group.values[index]}${suffix}</strong></div>`).join("")}
    </li>`).join("")}</ol>`;
}

// Several counts side by side for each person, as one stacked bar.
function stackedBars(rows, stacks) {
  const totals = rows.map((row) => stacks.reduce((sum, stack) => sum + (row[stack.key] || 0), 0));
  const visible = rows.filter((_, index) => totals[index] > 0);
  if (!visible.length) return noChartData("No activity in this period");
  const top = Math.max(1, ...totals);
  return `<ol class="bar-list">${rows.map((row, index) => totals[index] ? `<li>
      <div class="bar-list-label"><span>${escapeHtml(row.label)}</span><strong>${totals[index]}</strong></div>
      <div class="bar-list-track stacked" aria-hidden="true">${stacks.map((stack) => row[stack.key]
        ? `<span style="width:${row[stack.key] * 100 / top}%;background:${stack.colour}" title="${escapeAttr(stack.label)}: ${row[stack.key]}"></span>` : "").join("")}</div>
    </li>` : "").join("")}</ol>
    <ul class="chart-legend chart-legend-row">${stacks.map((stack) => `<li><i style="background:${stack.colour}"></i>${escapeHtml(stack.label)}</li>`).join("")}</ul>`;
}

// ---- The Reports page ----

function renderReportOverview(data) {
  const summary = data.monthlySummary || {}, kpi = data.kpi || {};
  const closedOrders = summary.closedWorkOrders || 0, openOrders = summary.openWorkOrders || 0;
  const otherTasks = Math.max(0, (kpi.assigned || 0) - (kpi.completed || 0) - (kpi.pending || 0));
  setHtml("[data-report-overview]", [
    chartCard("Audits", `${summary.audits || 0} in this report`, donutChart([
      { label: "Completed", value: summary.auditsCompleted || 0, colour: CHART.pass },
      { label: "Pending", value: summary.auditsPending || 0, colour: CHART.warn },
    ], `${summary.averageScore || 0}`, "avg score")),
    chartCard("Findings", `${summary.totalFindings || 0} recorded`, donutChart([
      { label: "Priority", value: summary.priorityFindings || 0, colour: CHART.fail },
      { label: "Non-priority", value: summary.nonPriorityFindings || 0, colour: CHART.info },
    ], summary.totalFindings || 0, "findings", `Open ${summary.outstandingFindings || 0} · <span class="${summary.overdueFindings ? "warn" : ""}">Overdue ${summary.overdueFindings || 0}</span>`)),
    chartCard("Work orders", `${openOrders + closedOrders} raised`, donutChart([
      { label: "Closed", value: closedOrders, colour: CHART.pass },
      { label: "Open", value: openOrders, colour: CHART.warn },
    ], `${summary.completionRate || 0}%`, "completed")),
    chartCard("Scheduled tasks", `${kpi.assigned || 0} assigned`, donutChart([
      { label: "Completed", value: kpi.completed || 0, colour: CHART.pass },
      { label: "Pending", value: kpi.pending || 0, colour: CHART.warn },
      ...(otherTasks ? [{ label: "In progress", value: otherTasks, colour: CHART.info }] : []),
    ], `${kpi.responseRate || 0}%`, "response")),
  ].join(""));
}

// "2026-10 · Priority" → month and class, stacked by month.
function priorityByMonth(rows) {
  const months = new Map();
  rows.forEach((row) => {
    const [month, kind = "Priority"] = String(row.label || "").split(" · ");
    const entry = months.get(month) || { label: month, priority: 0, other: 0 };
    entry[kind === "Priority" ? "priority" : "other"] += Number(row.count) || 0;
    months.set(month, entry);
  });
  return [...months.values()].sort((a, b) => a.label.localeCompare(b.label));
}

function renderReportTrends(data) {
  const charts = data.charts || {};
  const monthly = (charts.monthlyAuditTrend || []).map((row) => ({ label: row.month, bar: Number(row.audits) || 0, line: Math.round(Number(row.average_score) || 0) }))
    .sort((a, b) => a.label.localeCompare(b.label));
  const findings = priorityByMonth(charts.priorityTrend || []);
  setHtml("[data-report-trends]", [
    chartCard("Audits by month", "Completed audits each month, and their average score", columnLineChart(monthly, {
      barLabel: "Audits", lineLabel: "Average score", mark: reportScoring.passMark,
    }), true),
    chartCard("Findings by month", "Priority and non-priority findings raised each month", columnLineChart(findings, {
      barLabel: "Findings", stacks: [{ key: "priority", label: "Priority", colour: CHART.fail }, { key: "other", label: "Non-priority", colour: CHART.info }],
    }), true),
  ].join(""));
}

function renderReportOutlets(data) {
  const charts = data.charts || {};
  setHtml("[data-bars]", barList((data.rankings || []).map((row) => ({ label: row.outlet, value: Number(row.latest) || 0, colour: scoreColour(Number(row.latest) || 0),
    note: `Average ${row.average}/100 over ${row.audit_count} audit${row.audit_count === 1 ? "" : "s"}` })), { max: 100, suffix: "/100", mark: reportScoring.passMark }));
  const pairs = new Map();
  (charts.auditComparison || []).forEach((row) => {
    const [outlet, which] = String(row.label || "").split(" · ");
    const entry = pairs.get(outlet) || { label: outlet, values: [null, null] };
    entry.values[which === "Previous" ? 0 : 1] = Number(row.score) || 0;
    pairs.set(outlet, entry);
  });
  setHtml("[data-report-comparison]", pairedBars([...pairs.values()], ["Previous", "Current"], [CHART.muted, CHART.accent], { suffix: "/100" }));
}

function renderReportBreakdowns(charts) {
  const counts = (rows, colour) => (rows || []).map((row) => ({ label: row.label || "Unassigned", value: Number(row.count) || 0, colour }))
    .sort((a, b) => b.value - a.value);
  const percents = (rows) => (rows || []).map((row) => ({ label: row.label || "Unassigned", value: Number(row.score) || 0, colour: scoreColour(Number(row.score) || 0),
    note: `${row.completed || 0} of ${row.count || 0} closed` }));
  setHtml("[data-report-charts]", [
    chartCard("Findings by department", "Who the findings were assigned to", barList(counts(charts.findingsByDepartment, CHART.accent))),
    chartCard("Findings by location", "Where the findings were raised", barList(counts(charts.findingsByArea, CHART.accent))),
    chartCard("Findings by asset type", "What kind of item failed", barList(counts(charts.findingsByCategory, CHART.accent))),
    chartCard("Findings by priority", "Priority levels given", barList(counts(charts.findingsByPriority, CHART.fail))),
    chartCard("Work closed by department", "Share of work orders closed", barList(percents(charts.departmentPerformance), { max: 100, suffix: "%" })),
    chartCard("Work closed by location", "Share of work orders closed", barList(percents(charts.locationPerformance), { max: 100, suffix: "%" })),
    chartCard("Work closed by asset type", "Share of work orders closed", barList(percents(charts.categoryPerformance), { max: 100, suffix: "%" })),
    chartCard("Location scores by month", "Average audit score of each location", barList((charts.roomAuditTrend || []).map((row) => ({
      label: row.label, value: Number(row.score) || 0, colour: scoreColour(Number(row.score) || 0) })), { max: 100, suffix: "/100", mark: reportScoring.passMark })),
  ].join(""));
}

function renderTimeToActChart(rows) {
  setHtml("[data-time-to-act-chart]", barList((rows || []).filter((row) => row.count).map((row) => ({
    label: row.label, value: Number(row.averageSeconds) || 0, ghost: Number(row.longestSeconds) || 0, colour: CHART.accent,
    note: `${row.count} time${row.count === 1 ? "" : "s"} · longest ${durationText(row.longestSeconds)}`,
  })), { format: durationText }));
}

function renderPeopleChart(people) {
  setHtml("[data-kpi-people-chart]", stackedBars((people || []).map((row) => ({
    label: row.name, audits: row.audit_completed || 0, requests: (row.order_created || 0) + (row.request_declined || 0), orders: row.order_closed || 0,
  })), [
    { key: "audits", label: "Audits completed", colour: CHART.accent },
    { key: "requests", label: "Requests acted on", colour: CHART.info },
    { key: "orders", label: "Orders closed", colour: CHART.pass },
  ]));
}

function renderReportPage(data) {
  reportScoring = { ...reportScoring, ...(data.scoring || {}) };
  renderReportOverview(data);
  renderReportTrends(data);
  renderReportOutlets(data);
  renderReportBreakdowns(data.charts || {});
  renderTimeToActChart(data.timeToAct || []);
  renderPeopleChart(data.people || []);
}
