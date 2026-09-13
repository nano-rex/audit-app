async function loadApp() {
  if (!currentUser && !await requireLogin()) return;
  await loadBranding();

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
  loadFindings();
  loadWorkOrders();
  loadEquipment();
  loadUsers();
  loadNotifications();
}

wireAuth();
loadApp();
