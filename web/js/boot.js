async function loadApp() {
  if (!currentUser && !await requireLogin()) return;
  unitTexts.forEach((node) => {
    node.textContent = currentUnit;
  });

  if (questionSection) questionSection.textContent = "Ottotree Checklist";
  if (questionTitle) questionTitle.textContent = "Combined Store Readiness";
  if (questionCopy) questionCopy.textContent = "Complete the Mini Studio and Loudspeaker checks together.";

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
