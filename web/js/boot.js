async function loadApp() {
  if (!currentUser && !await requireLogin()) return;
  await loadBranding();

  if (questionSection) questionSection.textContent = `${currentUnit} Checklist`;
  if (questionTitle) questionTitle.textContent = `${currentUnit} Readiness`;
  if (questionCopy) questionCopy.textContent = "Complete the configured audit checks together.";

  await loadSetup();
  applyNavbarTabs();
  showTab(allowedAppTabs()[0]?.id || "today");
  updateSetupSelects();
  updateEquipmentLocationSelect();
  updateInspectionLocationSelect();
  setInspectionSignatures(inspectionSignatures());
  await loadChecklist();
  await restoreLastInspectionSession();
  await loadInspectionHistory();
  loadDashboard();
  loadAdmin();
  loadFindings();
  loadWorkOrders();
  loadEquipment();
  loadUsers();
  loadNotifications();
}

wireAuth();
loadApp();
