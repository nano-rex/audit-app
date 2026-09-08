package com.example.ottotreeaudit;

import android.app.Activity;
import android.app.AlertDialog;
import android.content.DialogInterface;
import android.graphics.Canvas;
import android.graphics.Color;
import android.graphics.Paint;
import android.graphics.RectF;
import android.graphics.Typeface;
import android.graphics.drawable.GradientDrawable;
import android.os.Bundle;
import android.text.InputType;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.widget.ArrayAdapter;
import android.widget.AdapterView;
import android.widget.AutoCompleteTextView;
import android.widget.Button;
import android.widget.CheckBox;
import android.widget.EditText;
import android.widget.HorizontalScrollView;
import android.widget.LinearLayout;
import android.widget.RadioButton;
import android.widget.RadioGroup;
import android.widget.ScrollView;
import android.widget.Spinner;
import android.widget.TextView;
import android.widget.Toast;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

public class MainActivity extends Activity {
    private static final int GREEN = Color.rgb(110, 146, 127);
    private static final int GREEN_LIGHT = Color.rgb(229, 240, 235);
    private static final int BORDER = Color.rgb(211, 211, 211);
    private static final int TEXT = Color.rgb(17, 29, 39);
    private static final int MUTED = Color.rgb(143, 153, 151);
    private static final int BLUE = Color.rgb(58, 129, 239);
    private static final int PURPLE = Color.rgb(124, 55, 232);
    private static final int ORANGE = Color.rgb(239, 137, 0);
    private static final int RED = Color.rgb(243, 64, 71);

    private LinearLayout root;
    private LinearLayout tabBar;
    private LinearLayout content;
    private final List<Button> tabButtons = new ArrayList<Button>();
    private AuditDatabase database;
    private int activeTab = 0;
    private String activeUnit = "Ottotree";
    private String currentRole = "admin";
    private String selectedLocationOutlet = "";
    private String equipmentSearch = "";
    private String equipmentFilterOutlet = "";
    private String equipmentFilterLocation = "";
    private String equipmentFilterType = "";
    private String equipmentFilterBrand = "";
    private String workOrderSearch = "";
    private String workOrderFilterOutlet = "";
    private String workOrderFilterLocation = "";
    private String workOrderFilterDepartment = "";
    private String workOrderFilterPriority = "";
    private String workOrderFilterStatus = "";
    private String departmentSearch = "";
    private String outletSearch = "";
    private String userSearch = "";
    private String userFilterRole = "";
    private String userFilterDepartment = "";
    private AuditDatabase.ScheduleRecord pendingInspectionSchedule;

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        database = new AuditDatabase(this);
        database.seedSchedulesIfEmpty();
        database.seedEquipmentIfEmpty();
        database.seedAdminIfEmpty();
        database.normalizeLoudspeakerOutlets();
        database.removeSampleAudits();
        buildShell();
        showTab(0);
    }

    private void buildShell() {
        root = new LinearLayout(this);
        root.setOrientation(LinearLayout.VERTICAL);
        root.setBackgroundColor(Color.rgb(248, 249, 247));
        setContentView(root);

        LinearLayout header = new LinearLayout(this);
        header.setOrientation(LinearLayout.VERTICAL);
        header.setPadding(dp(12), dp(10), dp(12), dp(8));
        header.setBackgroundColor(Color.WHITE);
        root.addView(header, new LinearLayout.LayoutParams(-1, -2));

        HorizontalScrollView actionScroller = new HorizontalScrollView(this);
        actionScroller.setHorizontalScrollBarEnabled(false);
        header.addView(actionScroller, new LinearLayout.LayoutParams(-1, -2));

        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        actions.setGravity(Gravity.CENTER_VERTICAL);
        actionScroller.addView(actions);

        LinearLayout brand = new LinearLayout(this);
        brand.setOrientation(LinearLayout.VERTICAL);
        brand.addView(label("Ottotree Audit", 22, GREEN, true));
        brand.addView(label("Loudspeaker & Mini Studio operations", 12, MUTED, false));
        actions.addView(brand, new LinearLayout.LayoutParams(dp(210), -2));

        tabBar = new LinearLayout(this);
        tabBar.setOrientation(LinearLayout.HORIZONTAL);
        tabBar.setGravity(Gravity.CENTER_VERTICAL);
        actions.addView(tabBar);

        rebuildTabs();

        ScrollView scroll = new ScrollView(this);
        root.addView(scroll, new LinearLayout.LayoutParams(-1, 0, 1));
        content = new LinearLayout(this);
        content.setOrientation(LinearLayout.VERTICAL);
        content.setPadding(dp(14), dp(18), dp(14), dp(18));
        scroll.addView(content, new ScrollView.LayoutParams(-1, -2));
    }

    private void addTab(String text, final int index) {
        Button tab = new Button(this);
        tab.setText(text);
        tab.setAllCaps(false);
        tab.setTextSize(13);
        tab.setMinHeight(0);
        tab.setMinimumHeight(0);
        tab.setPadding(dp(16), 0, dp(16), 0);
        tab.setTag(Integer.valueOf(index));
        tab.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showTab(index); }
        });
        tabButtons.add(tab);
        tabBar.addView(tab, new LinearLayout.LayoutParams(-2, dp(54)));
    }

    private void showTab(int index) {
        activeTab = index;
        for (int i = 0; i < tabButtons.size(); i++) {
            Button b = tabButtons.get(i);
            int tabIndex = ((Integer) b.getTag()).intValue();
            b.setTextColor(tabIndex == index ? TEXT : MUTED);
            b.setTypeface(Typeface.DEFAULT, tabIndex == index ? Typeface.BOLD : Typeface.NORMAL);
            b.setBackground(tabIndex == index ? shape(Color.WHITE, GREEN, dp(7)) : shape(Color.WHITE, Color.TRANSPARENT, 0));
        }
        content.removeAllViews();
        if (index == 0) showToday();
        if (index == 1) showInspections();
        if (index == 2) showWorkOrders();
        if (index == 3) showEquipment();
        if (index == 4) showReports();
        if (index == 5) showDepartments();
        if (index == 7) showUsers();
        if (index == 8) showAccount();
        if (index == 9) showOutlets();
    }

    private void rebuildTabs() {
        if (tabBar == null) return;
        tabBar.removeAllViews();
        tabButtons.clear();
        if ("manager".equals(currentRole) || "director".equals(currentRole)) {
            addTab("Reports", 4);
            addTab("Departments", 5);
            addTab("Outlets", 9);
            addTab("Users", 7);
            addTab("Account", 8);
            return;
        }
        addTab("Today", 0);
        addTab("Inspections", 1);
        addTab("Work Orders", 2);
        addTab("Equipment", 3);
        addTab("Reports", 4);
        if ("executive-admin".equals(currentRole)) addTab("Users", 7);
        addTab("Account", 8);
    }

    private void showToday() {
        AuditDatabase.AuditStats auditStats = database.getAuditStats(activeUnit);
        List<AuditDatabase.ScheduleRecord> schedules = database.getPendingSchedules(activeUnit, 5);

        LinearLayout hero = new LinearLayout(this);
        hero.setOrientation(LinearLayout.VERTICAL);
        hero.setPadding(dp(16), dp(16), dp(16), dp(16));
        hero.setBackground(shape(GREEN, Color.TRANSPARENT, dp(8)));
        content.addView(hero, margin(-1, -2, 0, 0, 0, dp(16)));
        hero.addView(label("FIELD WORKSPACE", 12, Color.rgb(226, 238, 232), true));
        hero.addView(label(activeUnit + " inspections for today", 23, Color.WHITE, true));

        LinearLayout quick = new LinearLayout(this);
        quick.setOrientation(LinearLayout.HORIZONTAL);
        quick.setPadding(0, dp(14), 0, 0);
        Button start = action("Start Inspection", Color.WHITE);
        start.setTextColor(GREEN);
        start.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showTab(1); }
        });
        quick.addView(start, weight());
        Button scan = outlineButton("Scan QR");
        scan.setTextColor(Color.WHITE);
        scan.setBackground(shape(GREEN, Color.WHITE, dp(7)));
        scan.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { toast("QR scanning will be added in the Equipment step"); }
        });
        quick.addView(scan, weight());
        hero.addView(quick);

        LinearLayout metrics = new LinearLayout(this);
        metrics.setOrientation(LinearLayout.VERTICAL);
        content.addView(metrics);
        int openTasks = schedules.size() + auditStats.below60;
        row(metrics, metric("SCHEDULED VISITS", String.valueOf(schedules.size()), GREEN), metric("PENDING UPLOADS", "0", GREEN));
        row(metrics, metric("FOLLOW-UP NEEDED", String.valueOf(auditStats.below60), ORANGE), metric("OPEN TASKS", String.valueOf(openTasks), GREEN));

        LinearLayout scheduled = panel("Scheduled Work");
        content.addView(scheduled, margin(-1, -2, 0, dp(16), 0, 0));
        if (schedules.size() == 0) {
            TextView empty = label("No scheduled work. Create a schedule to assign outlet checks.", 14, MUTED, false);
            empty.setGravity(Gravity.CENTER);
            empty.setPadding(0, dp(34), 0, dp(34));
            scheduled.addView(empty);
        } else {
            for (int i = 0; i < schedules.size(); i++) {
                AuditDatabase.ScheduleRecord row = schedules.get(i);
                scheduled.addView(scheduleRow(row));
            }
        }

        LinearLayout flow = panel("On-site Flow");
        content.addView(flow, margin(-1, -2, 0, dp(16), 0, 0));
        flow.addView(flowStep("1", "Scan outlet, room, or equipment QR"));
        flow.addView(flowStep("2", "Complete checklist by location"));
        flow.addView(flowStep("3", "Attach clear photos immediately"));
        flow.addView(flowStep("4", "Submit report or create follow-up work order"));
    }

    private void showWorkOrders() {
        LinearLayout header = panel("Work Orders");
        content.addView(header);
        final List<AuditDatabase.WorkOrderRecord> allRows = database.getWorkOrders(activeUnit, 200);
        header.addView(formLabel("Search"));
        final EditText search = input("Search work orders", false);
        search.setText(workOrderSearch);
        header.addView(search);
        header.addView(formLabel("Outlet"));
        final Spinner outletFilter = spinner(withAll("All outlets", outletOptions(activeUnit)));
        setSpinnerValue(outletFilter, workOrderFilterOutlet.length() == 0 ? "All outlets" : workOrderFilterOutlet);
        header.addView(outletFilter);
        header.addView(formLabel("Location"));
        final Spinner locationFilter = spinner(withAll("All locations", workOrderLocationOptions(allRows, workOrderFilterOutlet)));
        setSpinnerValue(locationFilter, workOrderFilterLocation.length() == 0 ? "All locations" : workOrderFilterLocation);
        header.addView(locationFilter);
        outletFilter.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
                String outletValue = allValue(outletFilter, "All outlets");
                ArrayAdapter<String> adapter = new ArrayAdapter<String>(MainActivity.this, android.R.layout.simple_spinner_dropdown_item,
                        withAll("All locations", workOrderLocationOptions(allRows, outletValue)));
                adapter.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);
                locationFilter.setAdapter(adapter);
            }
            @Override public void onNothingSelected(AdapterView<?> parent) { }
        });
        header.addView(formLabel("Department"));
        final Spinner departmentFilter = spinner(withAll("All departments", departmentOptions()));
        setSpinnerValue(departmentFilter, workOrderFilterDepartment.length() == 0 ? "All departments" : workOrderFilterDepartment);
        header.addView(departmentFilter);
        header.addView(formLabel("Priority"));
        final Spinner priorityFilter = spinner(new String[] {"All priorities", "High", "Medium", "Low"});
        setSpinnerValue(priorityFilter, workOrderFilterPriority.length() == 0 ? "All priorities" : workOrderFilterPriority);
        header.addView(priorityFilter);
        header.addView(formLabel("Status"));
        final Spinner statusFilter = spinner(new String[] {"All statuses", "Assigned", "In Progress", "Completed", "Verified"});
        setSpinnerValue(statusFilter, workOrderFilterStatus.length() == 0 ? "All statuses" : workOrderFilterStatus);
        header.addView(statusFilter);
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button apply = action("Apply", GREEN);
        apply.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                workOrderSearch = search.getText().toString().trim();
                workOrderFilterOutlet = allValue(outletFilter, "All outlets");
                workOrderFilterLocation = allValue(locationFilter, "All locations");
                workOrderFilterDepartment = allValue(departmentFilter, "All departments");
                workOrderFilterPriority = allValue(priorityFilter, "All priorities");
                workOrderFilterStatus = allValue(statusFilter, "All statuses");
                showTab(2);
            }
        });
        actions.addView(apply, weight());
        Button create = action("Add Work Order", GREEN);
        create.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showWorkOrderDialog(); }
        });
        actions.addView(create, weight());
        header.addView(actions, margin(-1, dp(42), 0, dp(12), 0, 0));

        LinearLayout list = panel("Work Orders");
        content.addView(list, margin(-1, -2, 0, dp(16), 0, 0));
        List<AuditDatabase.WorkOrderRecord> rows = filterWorkOrders(allRows);
        if (rows.size() == 0) {
            TextView empty = label("No work orders found. Adjust filters or add a new work order.", 14, MUTED, false);
            empty.setGravity(Gravity.CENTER);
            empty.setPadding(0, dp(34), 0, dp(34));
            list.addView(empty);
        } else {
            for (int i = 0; i < rows.size(); i++) {
                list.addView(workOrderRow(rows.get(i)));
            }
        }
    }

    private void showEquipment() {
        LinearLayout header = panel("Equipment Registry");
        content.addView(header);
        final List<AuditDatabase.EquipmentRecord> allRows = database.getEquipment(activeUnit, 200);
        header.addView(formLabel("Search"));
        final EditText search = input("Search equipment", false);
        search.setText(equipmentSearch);
        header.addView(search);
        header.addView(formLabel("Outlet"));
        final Spinner outletFilter = spinner(withAll("All outlets", outletOptions(activeUnit)));
        setSpinnerValue(outletFilter, equipmentFilterOutlet.length() == 0 ? "All outlets" : equipmentFilterOutlet);
        header.addView(outletFilter);
        header.addView(formLabel("Location"));
        final Spinner locationFilter = spinner(withAll("All locations", locationFilterOptions(allRows, equipmentFilterOutlet)));
        setSpinnerValue(locationFilter, equipmentFilterLocation.length() == 0 ? "All locations" : equipmentFilterLocation);
        header.addView(locationFilter);
        outletFilter.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
                String outletValue = allValue(outletFilter, "All outlets");
                ArrayAdapter<String> adapter = new ArrayAdapter<String>(MainActivity.this, android.R.layout.simple_spinner_dropdown_item,
                        withAll("All locations", locationFilterOptions(allRows, outletValue)));
                adapter.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);
                locationFilter.setAdapter(adapter);
            }
            @Override public void onNothingSelected(AdapterView<?> parent) { }
        });
        header.addView(formLabel("Type"));
        final Spinner typeFilter = spinner(withAll("All types", uniqueEquipmentValues(allRows, "type")));
        setSpinnerValue(typeFilter, equipmentFilterType.length() == 0 ? "All types" : equipmentFilterType);
        header.addView(typeFilter);
        header.addView(formLabel("Brand"));
        final Spinner brandFilter = spinner(withAll("All brands", uniqueEquipmentValues(allRows, "brand")));
        setSpinnerValue(brandFilter, equipmentFilterBrand.length() == 0 ? "All brands" : equipmentFilterBrand);
        header.addView(brandFilter);
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button apply = action("Apply", GREEN);
        apply.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                equipmentSearch = search.getText().toString().trim();
                equipmentFilterOutlet = allValue(outletFilter, "All outlets");
                equipmentFilterLocation = allValue(locationFilter, "All locations");
                equipmentFilterType = allValue(typeFilter, "All types");
                equipmentFilterBrand = allValue(brandFilter, "All brands");
                showTab(3);
            }
        });
        actions.addView(apply, weight());
        Button create = action("Add Equipment", GREEN);
        create.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showEquipmentDialog(); }
        });
        actions.addView(create, weight());
        header.addView(actions, margin(-1, dp(42), 0, dp(12), 0, 0));

        LinearLayout list = panel("Equipment");
        content.addView(list, margin(-1, -2, 0, dp(16), 0, 0));
        List<AuditDatabase.EquipmentRecord> rows = filterEquipment(allRows);
        if (rows.size() == 0) {
            TextView empty = label("No equipment found. Adjust filters or add a new equipment item.", 14, MUTED, false);
            empty.setGravity(Gravity.CENTER);
            empty.setPadding(0, dp(34), 0, dp(34));
            list.addView(empty);
        } else {
            for (int i = 0; i < rows.size(); i++) {
                list.addView(equipmentRow(rows.get(i)));
            }
        }
    }

    private void showDepartments() {
        showSetupRecords("Department", "Departments", "Create and remove departments used for work order ownership and user department assignment.", "Add Department");
    }

    private void showOutlets() {
        LinearLayout header = panel("Outlets");
        content.addView(header);
        header.addView(formLabel("Search"));
        final EditText search = input("Search outlets", false);
        search.setText(outletSearch);
        header.addView(search);
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button apply = action("Apply", GREEN);
        apply.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                outletSearch = search.getText().toString().trim();
                showTab(9);
            }
        });
        actions.addView(apply, weight());
        Button add = action("Add Outlet", GREEN);
        add.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showSetupDialog("Outlet"); }
        });
        actions.addView(add, weight());
        header.addView(actions, margin(-1, dp(42), 0, dp(12), 0, 0));

        LinearLayout outletList = panel("Outlets");
        content.addView(outletList, margin(-1, -2, 0, dp(16), 0, 0));
        List<AuditDatabase.AdminRecord> records = database.getAdminRecords();
        boolean hasOutlets = false;
        for (int i = 0; i < records.size(); i++) {
            AuditDatabase.AdminRecord record = records.get(i);
            if ("Outlet".equals(record.recordType) && matchesAdminSearch(record, outletSearch)) {
                hasOutlets = true;
                outletList.addView(adminRow(record));
            }
        }
        if (!hasOutlets) {
            outletList.addView(emptyText("No outlets created."));
        }

        LinearLayout locations = panel("Locations");
        content.addView(locations, margin(-1, -2, 0, dp(16), 0, 0));
        locations.addView(formLabel("Outlet"));
        final Spinner outlet = spinner(outletOptions(activeUnit));
        if (selectedLocationOutlet.length() == 0 && outlet.getCount() > 0) {
            selectedLocationOutlet = outlet.getItemAtPosition(0).toString();
        }
        setSpinnerValue(outlet, selectedLocationOutlet);
        locations.addView(outlet);
        Button addLocation = action("Add Location", GREEN);
        addLocation.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showLocationDialog(null); }
        });
        locations.addView(addLocation, margin(-1, dp(42), 0, dp(12), 0, 0));
        outlet.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
                String selected = parent.getItemAtPosition(position).toString();
                if (selected.equals(selectedLocationOutlet)) {
                    return;
                }
                selectedLocationOutlet = selected;
                showTab(9);
            }
            @Override public void onNothingSelected(AdapterView<?> parent) { }
        });
        List<AuditDatabase.LocationRecord> locationRows = database.getLocations(selectedLocationOutlet);
        if (locationRows.size() == 0) {
            locations.addView(emptyText("No locations created for this outlet."));
        } else {
            for (int i = 0; i < locationRows.size(); i++) {
                locations.addView(locationRow(locationRows.get(i)));
            }
        }
    }

    private void showSetupRecords(final String recordType, String title, String description, String actionLabel) {
        LinearLayout header = panel(title);
        content.addView(header);
        header.addView(formLabel("Search"));
        final EditText search = input("Search " + title.toLowerCase(), false);
        search.setText(departmentSearch);
        header.addView(search);
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button apply = action("Apply", GREEN);
        apply.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                departmentSearch = search.getText().toString().trim();
                showTab(5);
            }
        });
        actions.addView(apply, weight());
        Button add = action(actionLabel, GREEN);
        add.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showSetupDialog(recordType); }
        });
        actions.addView(add, weight());
        header.addView(actions, margin(-1, dp(42), 0, dp(12), 0, 0));

        LinearLayout list = panel(title);
        content.addView(list, margin(-1, -2, 0, dp(16), 0, 0));
        List<AuditDatabase.AdminRecord> records = database.getAdminRecords();
        boolean hasRows = false;
        for (int i = 0; i < records.size(); i++) {
            AuditDatabase.AdminRecord record = records.get(i);
            if (recordType.equals(record.recordType) && matchesAdminSearch(record, departmentSearch)) {
                hasRows = true;
                list.addView(adminRow(record));
            }
        }
        if (!hasRows) {
            list.addView(emptyText("No " + title.toLowerCase() + " created."));
        }
    }

    private void showInspections() {
        final LinearLayout form = panel("Guided Inspection");
        content.addView(form);
        form.addView(formLabel("Outlet"));
        final Spinner outlet = spinner(outletOptions(activeUnit));
        form.addView(outlet);
        final AuditDatabase.ScheduleRecord schedule = pendingInspectionSchedule;
        pendingInspectionSchedule = null;
        if (schedule != null) {
            setSpinnerValue(outlet, schedule.outlet);
        }
        form.addView(formLabel("Location"));
        final Spinner zone = spinner(locationOptions(outlet.getSelectedItem().toString()));
        if (schedule != null) {
            setSpinnerValue(zone, schedule.zone);
        }
        form.addView(zone);
        form.addView(formLabel("Inspector"));
        final EditText auditor = input("Who is inspecting?", false);
        if (schedule != null) {
            auditor.setText(schedule.auditor);
        }
        form.addView(auditor);
        form.addView(formLabel("Inspection Date"));
        final EditText date = input("Today", false);
        if (schedule != null) {
            date.setText(schedule.scheduledDate);
        }
        form.addView(date);

        final LinearLayout checklistHolder = new LinearLayout(this);
        checklistHolder.setOrientation(LinearLayout.VERTICAL);
        form.addView(checklistHolder, margin(-1, -2, 0, dp(14), 0, 0));
        final List<CheckBox> resultInputs = new ArrayList<CheckBox>();
        final List<EditText> noteInputs = new ArrayList<EditText>();
        final List<String> sectionInputs = new ArrayList<String>();
        final List<String> itemInputs = new ArrayList<String>();

        final Runnable renderChecklist = new Runnable() {
            @Override public void run() {
                checklistHolder.removeAllViews();
                noteInputs.clear();
                resultInputs.clear();
                sectionInputs.clear();
                itemInputs.clear();
                String selectedOutlet = outlet.getSelectedItem().toString();
                String selectedLocation = zone.getSelectedItem().toString();
                List<AuditDatabase.EquipmentRecord> equipment = database.getEquipmentForOutlet(selectedOutlet);
                int rendered = 0;
                for (int i = 0; i < equipment.size(); i++) {
                    final AuditDatabase.EquipmentRecord item = equipment.get(i);
                    if (!selectedLocation.equals(item.location)) continue;
                    rendered++;
                    LinearLayout row = card();
                    row.addView(label(item.type + " | " + item.code, 12, MUTED, true));
                    row.addView(label(item.name, 14, TEXT, true));
                    String[] criteria = inspectionCriteria(item);
                    for (int j = 0; j < criteria.length; j++) {
                        final String criterion = criteria[j];
                        final CheckBox passed = new CheckBox(MainActivity.this);
                        passed.setText(criterion);
                        passed.setTextColor(MUTED);
                        passed.setChecked(true);
                        row.addView(passed);
                        final EditText notes = input("Observation for this criterion", false);
                        notes.setEnabled(false);
                        row.addView(notes);
                        resultInputs.add(passed);
                        noteInputs.add(notes);
                        sectionInputs.add(item.name);
                        itemInputs.add(criterion);
                        passed.setOnClickListener(new View.OnClickListener() {
                            @Override public void onClick(View v) {
                                notes.setEnabled(!passed.isChecked());
                                passed.setTextColor(passed.isChecked() ? MUTED : RED);
                                if (!passed.isChecked()) {
                                    showPrefilledWorkOrder(selectedOutlet, selectedLocation, firstDepartment(),
                                            "High", item.name + " - " + criterion,
                                            "Equipment: " + item.name + "\nCode: " + item.code + "\nType: " + item.type + "\nFailed check: " + criterion);
                                } else {
                                    notes.setText("");
                                }
                            }
                        });
                    }
                    checklistHolder.addView(row, margin(-1, -2, 0, 0, 0, dp(12)));
                }
                if (rendered == 0) {
                    checklistHolder.addView(emptyText("No equipment is assigned to this location yet."));
                }
            }
        };
        renderChecklist.run();
        outlet.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
                zone.setAdapter(new ArrayAdapter<String>(MainActivity.this, android.R.layout.simple_spinner_dropdown_item, locationOptions(outlet.getSelectedItem().toString())));
                renderChecklist.run();
            }

            @Override public void onNothingSelected(AdapterView<?> parent) { }
        });
        zone.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
                renderChecklist.run();
            }

            @Override public void onNothingSelected(AdapterView<?> parent) { }
        });
        LinearLayout standard = panel("Inspection Standard");
        content.addView(standard, margin(-1, -2, 0, dp(16), 0, 0));
        standard.addView(flowStep("1", "One location per inspection"));
        standard.addView(flowStep("2", "Inspect each equipment item in that location"));
        standard.addView(flowStep("3", "Mark every criterion tick or cross"));
        standard.addView(flowStep("4", "Cross opens a prefilled work order request"));

        Button submit = action("Submit Inspection", GREEN);
        form.addView(submit, margin(-1, dp(42), 0, dp(10), 0, 0));
        submit.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                if (isInvalidSetupSelection(outlet, "outlets")) {
                    return;
                }
                List<AuditDatabase.InspectionItemInput> inputs = new ArrayList<AuditDatabase.InspectionItemInput>();
                for (int i = 0; i < itemInputs.size(); i++) {
                    boolean failed = !resultInputs.get(i).isChecked();
                    inputs.add(new AuditDatabase.InspectionItemInput(
                            sectionInputs.get(i),
                            itemInputs.get(i),
                            failed ? 0 : 100,
                            noteInputs.get(i).getText().toString().trim(),
                            failed ? "Missing" : "Uploaded",
                            failed));
                }
                database.saveInspection(activeUnit, outlet.getSelectedItem().toString(), zone.getSelectedItem().toString(), valueOrDefault(date, "Today"), valueOrDefault(auditor, "Unnamed Inspector"), inputs);
                toast("Inspection saved to SQLite");
                showTab(0);
            }
        });
    }

    private void showDashboard() {
        LinearLayout units = panel("Ottotree Coverage");
        content.addView(units, margin(-1, -2, 0, 0, 0, dp(16)));
        row(units, businessUnitCard("Loudspeaker", "Retail speaker display and outlet readiness", activeUnit.equals("Loudspeaker")),
                businessUnitCard("Mini Studio", "Headphone display and studio checklist", activeUnit.equals("Mini Studio")));

        LinearLayout metrics = new LinearLayout(this);
        metrics.setOrientation(LinearLayout.VERTICAL);
        content.addView(metrics);
        AuditDatabase.AuditStats stats = database.getAuditStats(activeUnit);
        row(metrics, metric("TOTAL AUDITS", String.valueOf(stats.total), GREEN), metric("AVG SCORE", stats.average + "/100", GREEN));
        row(metrics, metric("EXCELLENT (90+)", String.valueOf(stats.excellent), GREEN), metric("BELOW 60", String.valueOf(stats.below60), ORANGE));

        LinearLayout panel = panel(activeUnit + " Outlet Performance");
        content.addView(panel, margin(-1, -2, 0, dp(16), 0, 0));
        List<AuditDatabase.OutletSummary> outlets = database.getOutletSummaries(activeUnit);
        for (int i = 0; i < outlets.size(); i += 2) {
            View first = outletCard(outlets.get(i));
            View second = i + 1 < outlets.size() ? outletCard(outlets.get(i + 1)) : new SpaceView(this);
            row(panel, first, second);
        }

        LinearLayout recent = panel("Recent " + activeUnit + " Audits");
        content.addView(recent, margin(-1, -2, 0, dp(8), 0, 0));
        List<AuditDatabase.AuditRecord> recentRows = database.getRecentAudits(activeUnit, 5);
        for (int i = 0; i < recentRows.size(); i++) {
            AuditDatabase.AuditRecord row = recentRows.get(i);
            recent.addView(auditRow(row.outlet, row.branch, row.date, row.score, rating(row.score)));
        }
    }

    private void showLeaderboard() {
        LinearLayout panel = panel(activeUnit + " Outlet Rankings");
        content.addView(panel);
        List<AuditDatabase.OutletSummary> ranked = database.getOutletSummaries(activeUnit);
        sortByLatestDesc(ranked);
        int[] medals = new int[] { Color.rgb(248, 172, 48), Color.rgb(184, 194, 201), Color.rgb(221, 116, 50), GREEN };
        for (int i = 0; i < ranked.size(); i++) {
            panel.addView(rankingRow(i + 1, ranked.get(i), medals[Math.min(i, medals.length - 1)]));
        }
    }

    private void showCharts() {
        LinearLayout filter = panel("Filter by Date Range");
        content.addView(filter);
        filter.addView(formLabel("From Date"));
        filter.addView(input("mm / dd / yyyy", false));
        filter.addView(formLabel("To Date"));
        filter.addView(input("mm / dd / yyyy", false));
        LinearLayout buttons = new LinearLayout(this);
        buttons.setOrientation(LinearLayout.HORIZONTAL);
        buttons.addView(action("Apply Filter", GREEN), weight());
        buttons.addView(outlineButton("Reset"), weight());
        filter.addView(buttons);

        LinearLayout latest = panel("Latest " + activeUnit + " Scores by Outlet");
        content.addView(latest, margin(-1, -2, 0, dp(16), 0, 0));
        latest.addView(new BarChartView(this), new LinearLayout.LayoutParams(-1, dp(250)));

        LinearLayout dist = panel("Performance Distribution");
        content.addView(dist, margin(-1, -2, 0, dp(16), 0, 0));
        dist.addView(new DonutChartView(this), new LinearLayout.LayoutParams(-1, dp(250)));
    }

    private void showReports() {
        AuditDatabase.AuditStats auditStats = database.getAuditStats(activeUnit);
        AuditDatabase.KpiStats kpiStats = database.getKpiStats(activeUnit);
        List<AuditDatabase.WorkOrderRecord> workOrders = database.getWorkOrders(activeUnit, 20);
        int critical = 0;
        for (int i = 0; i < workOrders.size(); i++) {
            if ("High".equals(workOrders.get(i).priority)) critical++;
        }

        LinearLayout summary = panel(activeUnit + " Monthly Audit Report");
        content.addView(summary);
        row(summary, metric("AUDITS", String.valueOf(auditStats.total), GREEN), metric("AVERAGE SCORE", auditStats.average + "/100", GREEN));
        row(summary, metric("OPEN WORK ORDERS", String.valueOf(workOrders.size()), ORANGE), metric("ASSIGNED TASKS", String.valueOf(kpiStats.assigned), GREEN));
        row(summary, metric("COMPLETED", String.valueOf(kpiStats.completed), Color.rgb(0, 180, 110)), metric("PENDING", String.valueOf(kpiStats.pending), ORANGE));
        row(summary, metric("RESPONSE RATE", kpiStats.responseRate + "%", GREEN), metric("FOLLOW-UP", kpiStats.assigned == 0 ? "None" : "Active", GREEN));
        summary.addView(label("Export is available in the web version as CSV and JSON.", 13, MUTED, false));

        showLeaderboard();
        showCharts();

        LinearLayout issues = panel("Critical Issues");
        content.addView(issues, margin(-1, -2, 0, dp(16), 0, 0));
        if (critical == 0) {
            issues.addView(emptyText("No high priority work orders."));
        } else {
            for (int i = 0; i < workOrders.size(); i++) {
                if ("High".equals(workOrders.get(i).priority)) {
                    issues.addView(workOrderRow(workOrders.get(i)));
                }
            }
        }

        LinearLayout followUp = panel("Follow-up Overview");
        content.addView(followUp, margin(-1, -2, 0, dp(16), 0, 0));
        followUp.addView(emptyText(kpiStats.assigned == 0 ? "No audits assigned to captains yet." : "Scheduled visits are ready for follow-up review."));
    }

    private void showUsers() {
        LinearLayout header = panel("Users");
        content.addView(header);
        final List<AuditDatabase.UserRecord> allRows = database.getUsers();
        header.addView(formLabel("Search"));
        final EditText search = input("Search users", false);
        search.setText(userSearch);
        header.addView(search);
        header.addView(formLabel("Role"));
        final Spinner roleFilter = spinner(new String[] {"All roles", "Admin", "Executive Admin", "Manager", "Director"});
        setSpinnerValue(roleFilter, userFilterRole.length() == 0 ? "All roles" : userFilterRole);
        header.addView(roleFilter);
        header.addView(formLabel("Department"));
        final Spinner departmentFilter = spinner(withAll("All departments", departmentOptions()));
        setSpinnerValue(departmentFilter, userFilterDepartment.length() == 0 ? "All departments" : userFilterDepartment);
        header.addView(departmentFilter);
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button apply = action("Apply", GREEN);
        apply.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                userSearch = search.getText().toString().trim();
                userFilterRole = allValue(roleFilter, "All roles");
                userFilterDepartment = allValue(departmentFilter, "All departments");
                showTab(7);
            }
        });
        actions.addView(apply, weight());
        Button add = action("Add User", GREEN);
        add.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showUserDialog(); }
        });
        actions.addView(add, weight());
        header.addView(actions, margin(-1, dp(42), 0, dp(12), 0, 0));

        LinearLayout list = panel("User Access");
        content.addView(list, margin(-1, -2, 0, dp(16), 0, 0));
        List<AuditDatabase.UserRecord> rows = filterUsers(allRows);
        if (rows.size() == 0) {
            list.addView(emptyText("No users found. Adjust filters or add a user."));
        } else {
            for (int i = 0; i < rows.size(); i++) {
                list.addView(userRow(rows.get(i)));
            }
        }
    }

    private void showAccount() {
        LinearLayout panel = panel("Account");
        content.addView(panel);
        panel.addView(formLabel("Current Role"));
        final Spinner role = spinner(new String[] {"Admin", "Executive Admin", "Manager", "Director"});
        setSpinnerValue(role, roleLabel(currentRole));
        panel.addView(role);
        panel.addView(label("Admin has daily workflow access. Executive Admin also has Reports and Users. Manager and Director have Reports, Departments, Outlets, and Users.", 13, MUTED, false));
        Button apply = action("Apply Role", GREEN);
        apply.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                currentRole = roleKey(role.getSelectedItem().toString());
                rebuildTabs();
                showTab(("manager".equals(currentRole) || "director".equals(currentRole)) ? 6 : 0);
            }
        });
        panel.addView(apply, margin(-1, dp(42), 0, dp(12), 0, 0));
    }

    private void showUserDialog() {
        showUserDialog(null);
    }

    private void showUserDialog(final AuditDatabase.UserRecord existing) {
        final LinearLayout box = dialogBase(existing == null ? "Add User" : "Edit User");
        box.addView(formLabel("Name"));
        final EditText name = input("User name", false);
        box.addView(name);
        box.addView(formLabel("Email"));
        final EditText email = input("name@example.com", false);
        box.addView(email);
        box.addView(formLabel("Role"));
        final Spinner role = spinner(new String[] {"Admin", "Executive Admin", "Manager", "Director"});
        box.addView(role);
        box.addView(formLabel("Department"));
        final Spinner department = spinner(departmentOptions());
        box.addView(department);
        box.addView(formLabel("Title"));
        final EditText title = input("Job title", false);
        box.addView(title);
        box.addView(formLabel("Responsibilities"));
        final EditText responsibilities = input("Main responsibilities or ownership", false);
        responsibilities.setMinLines(2);
        box.addView(responsibilities);
        if (existing != null) {
            name.setText(existing.name);
            email.setText(existing.email);
            setSpinnerValue(role, existing.role);
            setSpinnerValue(department, existing.department);
            title.setText(existing.title);
            responsibilities.setText(existing.responsibilities);
        }
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button save = action(existing == null ? "Save User" : "Save Changes", GREEN);
        actions.addView(save, weight());
        actions.addView(outlineButton("Cancel"), weight());
        box.addView(actions);
        final AlertDialog d = dialog(box);
        save.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                if (isInvalidSetupSelection(department, "departments")) {
                    return;
                }
                if (existing == null) {
                    database.saveUser(valueOrDefault(name, "New User"), role.getSelectedItem().toString(),
                            valueOrDefault(email, "user@example.com"), department.getSelectedItem().toString(),
                            valueOrDefault(title, ""), responsibilities.getText().toString().trim());
                    toast("User saved");
                } else {
                    database.updateUser(existing.id, valueOrDefault(name, "New User"), role.getSelectedItem().toString(),
                            valueOrDefault(email, "user@example.com"), department.getSelectedItem().toString(),
                            valueOrDefault(title, ""), responsibilities.getText().toString().trim());
                    toast("User updated");
                }
                d.dismiss();
                showTab(7);
            }
        });
    }

    private void showNewAuditDialog() {
        final LinearLayout box = dialogBase("New Audit - Ottotree");
        box.addView(formLabel("Outlet"));
        final Spinner outlet = spinner(outletOptions(activeUnit, true));
        box.addView(outlet);
        box.addView(formLabel("Audit Date"));
        final EditText auditDate = input("mm / dd / yyyy", false);
        box.addView(auditDate);
        box.addView(formLabel("Auditor Name"));
        final EditText auditor = input("Who is auditing?", false);
        box.addView(auditor);

        TextView type = formLabel("Audit Type");
        box.addView(type);
        RadioGroup group = new RadioGroup(this);
        group.setOrientation(RadioGroup.HORIZONTAL);
        RadioButton standard = new RadioButton(this);
        standard.setText("Standard (1-4 Scale)");
        standard.setChecked(true);
        RadioButton quick = new RadioButton(this);
        quick.setText("Quick (Yes/No)");
        group.addView(standard);
        group.addView(quick);
        box.addView(group);

        TextView guide = label("SCORING GUIDE\n\n1 = Not Acceptable - Needs immediate follow-up\n2 = Below Standard - Issues to address\n3 = Good - Acceptable performance\n4 = Excellent - Meets all standards", 12, TEXT, false);
        guide.setPadding(dp(14), dp(12), dp(14), dp(12));
        guide.setBackground(shape(GREEN_LIGHT, Color.rgb(187, 213, 204), dp(7)));
        box.addView(guide, margin(-1, -2, 0, dp(12), 0, dp(12)));

        LinearLayout question = card();
        TextView qTitle = label("OTTOTREE CHECKLIST", 12, TEXT, true);
        question.addView(qTitle);
        question.addView(label("Combined Store Readiness", 13, TEXT, true));
        question.addView(label("Complete the Mini Studio and Loudspeaker checks together.", 13, MUTED, false));
        box.addView(question);
        box.addView(formLabel("Total Score"));
        final EditText scoreInput = input("0 - 100", false);
        scoreInput.setInputType(InputType.TYPE_CLASS_NUMBER);
        box.addView(scoreInput);
        box.addView(scoreSummary("TOTAL SCORE", "Saved from score field"));
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button save = action("Save Audit", GREEN);
        actions.addView(save, weight());
        actions.addView(outlineButton("Cancel"), weight());
        box.addView(actions);
        final AlertDialog d = dialog(box);
        save.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                String selectedOutlet = outlet.getSelectedItem().toString();
                if (isInvalidSetupSelection(outlet, "outlets")) {
                    return;
                }
                String date = valueOrDefault(auditDate, "Today");
                String name = valueOrDefault(auditor, "Unnamed Auditor");
                String type = standard.isChecked() ? "Standard" : "Quick";
                int score = scoreFrom(scoreInput);
                database.saveAudit(activeUnit, selectedOutlet, date, name, type, score);
                toast("Audit saved to SQLite");
                d.dismiss();
                showTab(activeTab);
            }
        });
    }

    private void showScheduleDialog() {
        showScheduleDialog(null);
    }

    private void showScheduleDialog(final AuditDatabase.ScheduleRecord existing) {
        final LinearLayout box = dialogBase("Schedule Audit Visit");
        box.addView(formLabel("Outlet"));
        final Spinner outlet = spinner(outletOptions(activeUnit, true));
        if (existing != null) {
            setSpinnerValue(outlet, existing.outlet);
        }
        box.addView(outlet);
        box.addView(formLabel("Location"));
        final Spinner zone = spinner(locationOptions(outlet.getSelectedItem().toString()));
        if (existing != null) {
            setSpinnerValue(zone, existing.zone);
        }
        box.addView(zone);
        outlet.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
                zone.setAdapter(new ArrayAdapter<String>(MainActivity.this, android.R.layout.simple_spinner_dropdown_item, locationOptions(outlet.getSelectedItem().toString())));
                if (existing != null && existing.outlet.equals(outlet.getSelectedItem().toString())) {
                    setSpinnerValue(zone, existing.zone);
                }
            }

            @Override public void onNothingSelected(AdapterView<?> parent) { }
        });
        box.addView(formLabel("Scheduled Date"));
        final EditText scheduledDate = input("08 / 28 / 2026", false);
        if (existing != null) scheduledDate.setText(existing.scheduledDate);
        box.addView(scheduledDate);
        box.addView(formLabel("Auditor"));
        final EditText auditor = input("Who will audit?", false);
        if (existing != null) auditor.setText(existing.auditor);
        box.addView(auditor);
        box.addView(formLabel("Remarks"));
        EditText remarks = input("Any notes...", false);
        if (existing != null) remarks.setText(existing.remarks);
        remarks.setMinLines(2);
        box.addView(remarks);
        box.addView(formLabel("Status"));
        final Spinner status = spinner(new String[] {"Pending", "Completed", "Cancelled"});
        if (existing != null) {
            setSpinnerValue(status, existing.status);
        }
        box.addView(status);
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button save = action(existing == null ? "Schedule" : "Save Changes", GREEN);
        actions.addView(save, weight());
        final AlertDialog[] dialogRef = new AlertDialog[1];
        if (existing != null) {
            Button delete = outlineButton("Delete");
            delete.setTextColor(RED);
            delete.setOnClickListener(new View.OnClickListener() {
                @Override public void onClick(View v) {
                    new AlertDialog.Builder(MainActivity.this)
                            .setTitle("Delete scheduled visit?")
                            .setMessage(existing.outlet + " - " + existing.scheduledDate)
                            .setPositiveButton("Delete", new DialogInterface.OnClickListener() {
                                @Override public void onClick(DialogInterface dialog, int which) {
                                    database.deleteSchedule(existing.id);
                                    toast("Schedule deleted");
                                    if (dialogRef[0] != null) dialogRef[0].dismiss();
                                    showTab(activeTab);
                                }
                            })
                            .setNegativeButton("Cancel", null)
                            .show();
                }
            });
            actions.addView(delete, weight());
        }
        actions.addView(outlineButton("Cancel"), weight());
        box.addView(actions);
        final AlertDialog d = dialog(box);
        dialogRef[0] = d;
        save.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                String selectedOutlet = outlet.getSelectedItem().toString();
                if (isInvalidSetupSelection(outlet, "outlets")) {
                    return;
                }
                if (isInvalidSetupSelection(zone, "locations")) {
                    return;
                }
                if (existing == null) {
                    database.saveSchedule(activeUnit, selectedOutlet, zone.getSelectedItem().toString(), valueOrDefault(scheduledDate, "08 / 28 / 2026"), valueOrDefault(auditor, "Unassigned"), remarks.getText().toString().trim());
                    toast("Schedule saved to SQLite");
                } else {
                    database.updateSchedule(existing.id, selectedOutlet, zone.getSelectedItem().toString(), valueOrDefault(scheduledDate, "08 / 28 / 2026"), valueOrDefault(auditor, "Unassigned"), remarks.getText().toString().trim(), status.getSelectedItem().toString());
                    toast("Schedule updated");
                }
                d.dismiss();
                showTab(activeTab);
            }
        });
    }

    private void showWorkOrderDialog() {
        showWorkOrderDialog(null);
    }

    private void showWorkOrderDialog(final AuditDatabase.WorkOrderRecord existing) {
        showWorkOrderDialog(existing, "", "", "", "", "", "");
    }

    private void showPrefilledWorkOrder(String outletValue, String locationValue, String departmentValue,
                                        String priorityValue, String titleValue, String descriptionValue) {
        showWorkOrderDialog(null, outletValue, locationValue, departmentValue, priorityValue, titleValue, descriptionValue);
    }

    private void showWorkOrderDialog(final AuditDatabase.WorkOrderRecord existing, String outletValue,
                                     String locationValue, String departmentValue, String priorityValue,
                                     String titleValue, String descriptionValue) {
        final LinearLayout box = dialogBase(existing == null ? "Create Work Order" : "Edit Work Order");
        box.addView(formLabel("Outlet"));
        final Spinner outlet = spinner(outletOptions(activeUnit));
        if (existing != null) setSpinnerValue(outlet, existing.outlet);
        else if (outletValue.length() > 0) setSpinnerValue(outlet, outletValue);
        box.addView(outlet);
        box.addView(formLabel("Location"));
        final EditText zone = input("Location name", false);
        if (existing != null) zone.setText(existing.zone);
        else if (locationValue.length() > 0) zone.setText(locationValue);
        box.addView(zone);
        box.addView(formLabel("Department"));
        final Spinner requestType = spinner(departmentOptions());
        if (existing != null) setSpinnerValue(requestType, existing.requestType);
        else if (departmentValue.length() > 0) setSpinnerValue(requestType, departmentValue);
        box.addView(requestType);
        box.addView(formLabel("Priority"));
        final Spinner priority = spinner(new String[] {"High", "Medium", "Low"});
        priority.setSelection(1);
        if (existing != null) setSpinnerValue(priority, existing.priority);
        else if (priorityValue.length() > 0) setSpinnerValue(priority, priorityValue);
        box.addView(priority);
        box.addView(formLabel("Status"));
        final Spinner status = spinner(new String[] {"Assigned", "In Progress", "Completed", "Verified"});
        if (existing != null) setSpinnerValue(status, existing.status);
        box.addView(status);
        box.addView(formLabel("Assignee"));
        final EditText assignee = input("Technical Support", false);
        if (existing != null) assignee.setText(existing.assignee);
        box.addView(assignee);
        box.addView(formLabel("Issue Title"));
        final EditText title = input("What needs follow-up?", false);
        if (existing != null) title.setText(existing.title);
        else if (titleValue.length() > 0) title.setText(titleValue);
        box.addView(title);
        box.addView(formLabel("Description"));
        final EditText description = input("Troubleshooting detail or outlet confirmation needed", false);
        description.setMinLines(2);
        if (existing != null) description.setText(existing.description);
        else if (descriptionValue.length() > 0) description.setText(descriptionValue);
        box.addView(description);

        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button save = action(existing == null ? "Create" : "Save Changes", GREEN);
        actions.addView(save, weight());
        actions.addView(outlineButton("Cancel"), weight());
        box.addView(actions);
        final AlertDialog d = dialog(box);
        save.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                if (isInvalidSetupSelection(outlet, "outlets") || isInvalidSetupSelection(requestType, "departments")) {
                    return;
                }
                if (existing == null) {
                    database.saveManualWorkOrder(activeUnit, outlet.getSelectedItem().toString(), valueOrDefault(zone, "Unassigned"),
                            requestType.getSelectedItem().toString(), priority.getSelectedItem().toString(),
                            valueOrDefault(title, "Work order"), description.getText().toString().trim(),
                            valueOrDefault(assignee, "Technical Support"));
                    toast("Work order saved to SQLite");
                } else {
                    database.updateWorkOrder(existing.id, outlet.getSelectedItem().toString(), valueOrDefault(zone, "Unassigned"),
                            requestType.getSelectedItem().toString(), priority.getSelectedItem().toString(),
                            valueOrDefault(title, "Work order"), description.getText().toString().trim(),
                            valueOrDefault(assignee, "Technical Support"), status.getSelectedItem().toString());
                    toast("Work order updated");
                }
                d.dismiss();
                showTab(2);
            }
        });
    }

    private void showEquipmentDialog() {
        showEquipmentDialog(null);
    }

    private void showEquipmentDialog(final AuditDatabase.EquipmentRecord existing) {
        final LinearLayout box = dialogBase(existing == null ? "Register Equipment" : "Edit Equipment");
        box.addView(formLabel("Name"));
        final AutoCompleteTextView name = new AutoCompleteTextView(this);
        name.setHint("Table, chair, light, TV");
        name.setTextColor(TEXT);
        name.setSingleLine(true);
        final List<AuditDatabase.EquipmentRecord> templates = database.getEquipment(activeUnit, 500);
        name.setAdapter(new ArrayAdapter<String>(this, android.R.layout.simple_dropdown_item_1line, equipmentNames(templates)));
        box.addView(name);
        box.addView(formLabel("Code"));
        final EditText code = input("Equipment code", false);
        box.addView(code);
        box.addView(formLabel("Outlet"));
        final Spinner outlet = spinner(outletOptions(activeUnit));
        if (existing != null) setSpinnerValue(outlet, existing.outlet);
        box.addView(outlet);
        box.addView(formLabel("Location"));
        final Spinner location = spinner(locationOptions(outlet.getSelectedItem().toString()));
        if (existing != null) setSpinnerValue(location, existing.location);
        box.addView(location);
        outlet.setOnItemSelectedListener(new AdapterView.OnItemSelectedListener() {
            @Override public void onItemSelected(AdapterView<?> parent, View view, int position, long id) {
                ArrayAdapter<String> adapter = new ArrayAdapter<String>(MainActivity.this, android.R.layout.simple_spinner_dropdown_item,
                        locationOptions(parent.getItemAtPosition(position).toString()));
                adapter.setDropDownViewResource(android.R.layout.simple_spinner_dropdown_item);
                location.setAdapter(adapter);
            }
            @Override public void onNothingSelected(AdapterView<?> parent) { }
        });
        box.addView(formLabel("Type"));
        final EditText type = input("Furniture, lighting, display, AV", false);
        box.addView(type);
        box.addView(formLabel("Operational Status"));
        final Spinner status = spinner(new String[] {"Operational", "Needs Attention", "Out of Service", "Replace"});
        box.addView(status);
        box.addView(formLabel("Brand"));
        final EditText brand = input("Brand", false);
        box.addView(brand);
        box.addView(formLabel("Model"));
        final EditText model = input("Model", false);
        box.addView(model);
        box.addView(formLabel("Serial Number"));
        final EditText serial = input("Serial number", false);
        box.addView(serial);
        box.addView(formLabel("Installation Date"));
        final EditText installationDate = input("YYYY-MM-DD", false);
        box.addView(installationDate);
        box.addView(formLabel("Description"));
        final EditText description = input("Condition, usage, compatibility concern, or replacement note", false);
        description.setMinLines(2);
        box.addView(description);
        box.addView(formLabel("Inspection Criteria"));
        final EditText criteria = input("One criterion per line", false);
        criteria.setMinLines(4);
        criteria.setText(AuditDatabase.defaultInspectionCriteria());
        box.addView(criteria);
        name.setOnItemClickListener(new AdapterView.OnItemClickListener() {
            @Override public void onItemClick(AdapterView<?> parent, View view, int position, long id) {
                AuditDatabase.EquipmentRecord template = equipmentTemplate(templates, name.getText().toString());
                if (template == null) return;
                type.setText(template.type);
                brand.setText(template.brand);
                description.setText(template.description);
                criteria.setText(template.inspectionCriteria);
            }
        });
        if (existing != null) {
            name.setText(existing.name);
            code.setText(existing.code);
            type.setText(existing.type);
            setSpinnerValue(status, existing.operationalStatus);
            brand.setText(existing.brand);
            model.setText(existing.model);
            serial.setText(existing.serialNumber);
            installationDate.setText(existing.installationDate);
            description.setText(existing.description);
            criteria.setText(existing.inspectionCriteria);
        }

        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button save = action(existing == null ? "Save Equipment" : "Save Changes", GREEN);
        actions.addView(save, weight());
        actions.addView(outlineButton("Cancel"), weight());
        box.addView(actions);
        final AlertDialog d = dialog(box);
        save.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                if (isInvalidSetupSelection(outlet, "outlets") || isInvalidSetupSelection(location, "locations")) {
                    return;
                }
                String equipmentCode = valueOrDefault(code, "EQ-" + System.currentTimeMillis());
                if (existing == null) {
                    database.saveEquipment(activeUnit, outlet.getSelectedItem().toString(), location.getSelectedItem().toString(),
                            valueOrDefault(name, equipmentCode), description.getText().toString().trim(),
                            valueOrDefault(type, "Equipment"), status.getSelectedItem().toString(), equipmentCode,
                            valueOrDefault(model, ""), valueOrDefault(serial, ""), valueOrDefault(brand, ""),
                            valueOrDefault(installationDate, "Today"), valueOrDefault(criteria, AuditDatabase.defaultInspectionCriteria()));
                    toast("Equipment saved to SQLite");
                } else {
                    database.updateEquipment(existing.id, activeUnit, outlet.getSelectedItem().toString(), location.getSelectedItem().toString(),
                            valueOrDefault(name, equipmentCode), description.getText().toString().trim(),
                            valueOrDefault(type, "Equipment"), status.getSelectedItem().toString(), equipmentCode,
                            valueOrDefault(model, ""), valueOrDefault(serial, ""), valueOrDefault(brand, ""),
                            valueOrDefault(installationDate, "Today"), valueOrDefault(criteria, AuditDatabase.defaultInspectionCriteria()));
                    toast("Equipment updated");
                }
                d.dismiss();
                showTab(3);
            }
        });
    }

    private void showSetupDialog(final String recordType) {
        showSetupDialog(recordType, null);
    }

    private void showSetupDialog(final String recordType, final AuditDatabase.AdminRecord existing) {
        final boolean isDepartment = "Department".equals(recordType);
        final LinearLayout box = dialogBase(existing == null ? (isDepartment ? "Add Department" : "Add Outlet") : (isDepartment ? "Edit Department" : "Edit Outlet"));
        box.addView(formLabel("Code"));
        final EditText name = input(isDepartment ? "Department code" : "Outlet code", false);
        if (existing != null) name.setText(existing.name);
        box.addView(name);
        box.addView(formLabel(isDepartment ? "Group" : "Location"));
        final EditText parent = input(isDepartment ? "Work Orders" : "Mall, branch, city, or floor", false);
        if (existing != null) parent.setText(existing.parent);
        box.addView(parent);
        box.addView(formLabel(isDepartment ? "Responsibilities" : "Description"));
        final EditText detail = input(isDepartment ? "What this department handles" : "Outlet description or operating note", false);
        detail.setMinLines(2);
        if (existing != null) detail.setText(existing.detail);
        box.addView(detail);

        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button save = action(existing == null ? (isDepartment ? "Save Department" : "Save Outlet") : "Save Changes", GREEN);
        actions.addView(save, weight());
        actions.addView(outlineButton("Cancel"), weight());
        box.addView(actions);
        box.addView(masterDataHelp(), margin(-1, -2, 0, dp(14), 0, 0));
        final AlertDialog d = dialog(box);
        save.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                if (existing == null) {
                    database.saveAdminRecord(recordType, valueOrDefault(name, "New record").toUpperCase(),
                            valueOrDefault(parent, ""), detail.getText().toString().trim());
                } else {
                    database.updateAdminRecord(existing.id, recordType, valueOrDefault(name, "New record").toUpperCase(),
                            valueOrDefault(parent, ""), detail.getText().toString().trim());
                }
                toast(recordType + " saved");
                d.dismiss();
                showTab(isDepartment ? 5 : 9);
            }
        });
    }

    private void showLocationDialog(final AuditDatabase.LocationRecord existing) {
        final LinearLayout box = dialogBase(existing == null ? "Add Location" : "Edit Location");
        box.addView(formLabel("Location Name"));
        final EditText name = input("Location name", false);
        box.addView(name);
        box.addView(formLabel("Size"));
        final EditText size = input("Area, room size, or capacity", false);
        box.addView(size);
        box.addView(formLabel("Equipment"));
        final List<CheckBox> equipmentChecks = new ArrayList<CheckBox>();
        final List<AuditDatabase.EquipmentRecord> equipment = database.getEquipmentForOutlet(selectedLocationOutlet);
        for (int i = 0; i < equipment.size(); i++) {
            AuditDatabase.EquipmentRecord item = equipment.get(i);
            CheckBox check = new CheckBox(this);
            check.setText(item.name + " - " + item.code);
            check.setTextColor(TEXT);
            check.setChecked(existing != null && existing.name.equals(item.location));
            equipmentChecks.add(check);
            box.addView(check);
        }
        if (existing != null) {
            name.setText(existing.name);
            size.setText(existing.size);
        }
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button save = action(existing == null ? "Save Location" : "Save Changes", GREEN);
        actions.addView(save, weight());
        actions.addView(outlineButton("Cancel"), weight());
        box.addView(actions);
        final AlertDialog d = dialog(box);
        save.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                String locationName = valueOrDefault(name, "New Location");
                if (existing == null) {
                    database.saveLocation(selectedLocationOutlet, locationName, valueOrDefault(size, ""));
                } else {
                    database.updateLocation(existing.id, selectedLocationOutlet, locationName, valueOrDefault(size, ""));
                }
                for (int i = 0; i < equipmentChecks.size(); i++) {
                    if (equipmentChecks.get(i).isChecked()) {
                        database.assignEquipmentLocation(equipment.get(i).id, selectedLocationOutlet, locationName);
                    }
                }
                toast("Location saved");
                d.dismiss();
                showTab(9);
            }
        });
    }

    private void showCaptainDialog() {
        final LinearLayout box = dialogBase("Captain Login");
        box.addView(formLabel("Select Outlet"));
        final Spinner outlet = spinner(outletOptions(activeUnit, true));
        box.addView(outlet);
        box.addView(formLabel("Enter PIN Code"));
        EditText pin = input("1113", true);
        pin.setInputType(InputType.TYPE_CLASS_NUMBER | InputType.TYPE_NUMBER_VARIATION_PASSWORD);
        box.addView(pin);
        TextView guide = label("PIN Guide:\nUse the outlet captain PIN assigned for the selected outlet.", 12, TEXT, true);
        guide.setPadding(dp(14), dp(12), dp(14), dp(12));
        guide.setBackground(shape(GREEN_LIGHT, Color.rgb(187, 213, 204), dp(7)));
        box.addView(guide, margin(-1, -2, 0, dp(12), 0, dp(12)));
        box.addView(formLabel("Your Name"));
        final EditText captainName = input("Enter your name...", false);
        box.addView(captainName);
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button login = action("Login", PURPLE);
        actions.addView(login, weight());
        actions.addView(outlineButton("Cancel"), weight());
        box.addView(actions);
        final AlertDialog d = dialog(box);
        login.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                String selectedOutlet = outlet.getSelectedItem().toString();
                if (isInvalidSetupSelection(outlet, "outlets")) {
                    return;
                }
                database.saveCaptainLogin(selectedOutlet, valueOrDefault(captainName, "Unnamed Captain"));
                toast("Captain login saved to SQLite");
                d.dismiss();
            }
        });
    }

    private LinearLayout dialogBase(String title) {
        LinearLayout box = new LinearLayout(this);
        box.setOrientation(LinearLayout.VERTICAL);
        box.setPadding(dp(18), dp(16), dp(18), dp(18));
        box.addView(label(title, 18, TEXT, true));
        return box;
    }

    private AlertDialog dialog(LinearLayout box) {
        ScrollView scroll = new ScrollView(this);
        scroll.addView(box);
        AlertDialog d = new AlertDialog.Builder(this).setView(scroll).create();
        d.setOnShowListener(new DialogInterface.OnShowListener() {
            @Override public void onShow(DialogInterface dialog) {
                Window w = d.getWindow();
                if (w != null) w.setLayout((int) (getResources().getDisplayMetrics().widthPixels * 0.94f), ViewGroup.LayoutParams.WRAP_CONTENT);
            }
        });
        d.show();
        return d;
    }

    private LinearLayout panel(String title) {
        LinearLayout p = new LinearLayout(this);
        p.setOrientation(LinearLayout.VERTICAL);
        p.setPadding(dp(16), dp(18), dp(16), dp(16));
        p.setBackground(shape(Color.WHITE, BORDER, dp(8)));
        p.addView(label(title, 18, TEXT, true), margin(-1, -2, 0, 0, 0, dp(14)));
        return p;
    }

    private LinearLayout metric(String title, String value, int color) {
        LinearLayout c = card();
        c.addView(label(title, 12, MUTED, true));
        c.addView(label(value, 26, color, true));
        return c;
    }

    private LinearLayout outletCard(AuditDatabase.OutletSummary o) {
        LinearLayout c = card();
        c.addView(label(o.code, 14, TEXT, true));
        c.addView(label(String.valueOf(o.average), 25, BLUE, true));
        c.addView(label("Average | " + o.auditCount + (o.auditCount == 1 ? " audit" : " audits"), 12, MUTED, false));
        return c;
    }

    private LinearLayout businessUnitCard(String name, String detail, boolean selected) {
        LinearLayout c = card();
        c.setBackground(shape(selected ? GREEN_LIGHT : Color.WHITE, selected ? GREEN : BORDER, dp(7)));
        c.addView(label(name, 16, selected ? GREEN : TEXT, true));
        c.addView(label(detail, 12, MUTED, false));
        c.addView(label(selected ? "Active checklist" : "Available checklist", 12, selected ? GREEN : MUTED, true));
        return c;
    }

    private LinearLayout auditRow(String outlet, String branch, String date, int score, String rating) {
        LinearLayout row = card();
        row.setOrientation(LinearLayout.HORIZONTAL);
        LinearLayout left = new LinearLayout(this);
        left.setOrientation(LinearLayout.VERTICAL);
        left.addView(label(outlet, 15, TEXT, true));
        left.addView(label(branch + " - " + date, 12, MUTED, false));
        row.addView(left, new LinearLayout.LayoutParams(0, -2, 1));
        LinearLayout right = new LinearLayout(this);
        right.setGravity(Gravity.RIGHT);
        right.setOrientation(LinearLayout.VERTICAL);
        right.addView(label(String.valueOf(score), 21, score >= 90 ? Color.rgb(36, 195, 98) : BLUE, true));
        right.addView(label(rating, 12, MUTED, false));
        row.addView(right);
        return row;
    }

    private LinearLayout scheduleRow(final AuditDatabase.ScheduleRecord record) {
        LinearLayout row = card();
        row.setOrientation(LinearLayout.HORIZONTAL);
        LinearLayout left = new LinearLayout(this);
        left.setOrientation(LinearLayout.VERTICAL);
        left.addView(label(record.outlet + " - " + record.scheduledDate, 15, TEXT, true));
        left.addView(label(record.auditor + " | " + record.zone + " | " + (record.remarks.length() == 0 ? "No remarks" : record.remarks), 12, MUTED, false));
        row.addView(left, new LinearLayout.LayoutParams(0, -2, 1));
        LinearLayout right = new LinearLayout(this);
        right.setOrientation(LinearLayout.VERTICAL);
        right.setGravity(Gravity.RIGHT);
        TextView status = label(record.status, 13, GREEN, true);
        status.setGravity(Gravity.RIGHT);
        right.addView(status);
        LinearLayout actions = new LinearLayout(this);
        actions.setOrientation(LinearLayout.HORIZONTAL);
        Button open = action("Open", GREEN);
        open.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                pendingInspectionSchedule = record;
                showTab(1);
            }
        });
        actions.addView(open);
        Button edit = outlineButton("Edit");
        edit.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showScheduleDialog(record); }
        });
        actions.addView(edit);
        right.addView(actions);
        row.addView(right);
        return row;
    }

    private LinearLayout workOrderRow(final AuditDatabase.WorkOrderRecord record) {
        LinearLayout row = card();
        row.setOrientation(LinearLayout.HORIZONTAL);
        LinearLayout left = new LinearLayout(this);
        left.setOrientation(LinearLayout.VERTICAL);
        left.addView(label("#" + record.id + " " + record.title, 15, TEXT, true));
        left.addView(label(record.outlet + " | " + record.zone + " | " + record.requestType + " | " + record.assignee, 12, MUTED, false));
        left.addView(label(record.outletConfirmed ? "Outlet confirmed" : "Awaiting outlet confirmation", 12, MUTED, false));
        row.addView(left, new LinearLayout.LayoutParams(0, -2, 1));
        LinearLayout right = new LinearLayout(this);
        right.setGravity(Gravity.RIGHT);
        right.setOrientation(LinearLayout.VERTICAL);
        right.addView(label(record.priority, 18, "High".equals(record.priority) ? ORANGE : GREEN, true));
        right.addView(label(record.status, 12, MUTED, false));
        Button edit = outlineButton("Edit");
        edit.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showWorkOrderDialog(record); }
        });
        right.addView(edit);
        Button delete = outlineButton("Delete");
        delete.setTextColor(RED);
        delete.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                database.deleteWorkOrder(record.id);
                toast("Work order deleted");
                showTab(2);
            }
        });
        right.addView(delete);
        row.addView(right);
        return row;
    }

    private LinearLayout equipmentRow(AuditDatabase.EquipmentRecord record) {
        LinearLayout row = card();
        row.setOrientation(LinearLayout.HORIZONTAL);
        LinearLayout left = new LinearLayout(this);
        left.setOrientation(LinearLayout.VERTICAL);
        left.addView(label(record.name + " - " + record.type, 15, TEXT, true));
        left.addView(label(record.outlet + " | " + record.location + " | " + record.code, 12, MUTED, false));
        left.addView(label(record.brand + " " + record.model + " | Serial: " +
                (record.serialNumber.length() == 0 ? "No serial" : record.serialNumber), 12, MUTED, false));
        if (record.description.length() > 0) {
            left.addView(label(record.description, 12, MUTED, false));
        }
        row.addView(left, new LinearLayout.LayoutParams(0, -2, 1));
        LinearLayout right = new LinearLayout(this);
        right.setGravity(Gravity.RIGHT);
        right.setOrientation(LinearLayout.VERTICAL);
        int color = "Replace".equals(record.operationalStatus) || "Out of Service".equals(record.operationalStatus) ? ORANGE : ("Needs Attention".equals(record.operationalStatus) ? ORANGE : GREEN);
        right.addView(label(record.operationalStatus, 16, color, true));
        right.addView(label(record.installationDate, 12, MUTED, false));
        Button edit = outlineButton("Edit");
        edit.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showEquipmentDialog(record); }
        });
        right.addView(edit);
        Button delete = outlineButton("Delete");
        delete.setTextColor(RED);
        delete.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                database.deleteEquipment(record.id);
                toast("Equipment deleted");
                showTab(3);
            }
        });
        right.addView(delete);
        row.addView(right);
        return row;
    }

    private LinearLayout adminRow(final AuditDatabase.AdminRecord record) {
        LinearLayout row = card();
        row.setOrientation(LinearLayout.HORIZONTAL);
        LinearLayout left = new LinearLayout(this);
        left.setOrientation(LinearLayout.VERTICAL);
        left.addView(label(record.recordType + " " + record.name, 15, TEXT, true));
        left.addView(label((record.parent.length() == 0 ? "No location/group" : record.parent) + " | " +
                (record.detail.length() == 0 ? "No description" : record.detail), 12, MUTED, false));
        row.addView(left, new LinearLayout.LayoutParams(0, -2, 1));
        Button edit = outlineButton("Edit");
        edit.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                showSetupDialog(record.recordType, record);
            }
        });
        row.addView(edit);
        Button delete = outlineButton("Delete");
        delete.setTextColor(RED);
        delete.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                database.deleteAdminRecord(record.id);
                toast("Setup record deleted");
                showTab(activeTab);
            }
        });
        row.addView(delete);
        return row;
    }

    private LinearLayout userRow(AuditDatabase.UserRecord record) {
        LinearLayout row = card();
        row.setOrientation(LinearLayout.HORIZONTAL);
        LinearLayout left = new LinearLayout(this);
        left.setOrientation(LinearLayout.VERTICAL);
        left.addView(label(record.name + " - " + record.role, 15, TEXT, true));
        left.addView(label(record.email + " | " + (record.department.length() == 0 ? "No department" : record.department) + " | " +
                (record.title.length() == 0 ? "No title" : record.title), 12, MUTED, false));
        left.addView(label(record.responsibilities.length() == 0 ? "No responsibilities" : record.responsibilities, 12, MUTED, false));
        row.addView(left, new LinearLayout.LayoutParams(0, -2, 1));
        Button edit = outlineButton("Edit");
        edit.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showUserDialog(record); }
        });
        row.addView(edit);
        Button delete = outlineButton("Delete");
        delete.setTextColor(RED);
        delete.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                database.deleteUser(record.id);
                toast("User deleted");
                showTab(activeTab);
            }
        });
        row.addView(delete);
        return row;
    }

    private LinearLayout locationRow(final AuditDatabase.LocationRecord record) {
        LinearLayout row = card();
        row.setOrientation(LinearLayout.HORIZONTAL);
        LinearLayout left = new LinearLayout(this);
        left.setOrientation(LinearLayout.VERTICAL);
        left.addView(label(record.name, 15, TEXT, true));
        left.addView(label(record.size.length() == 0 ? "No size" : record.size, 12, MUTED, false));
        row.addView(left, new LinearLayout.LayoutParams(0, -2, 1));
        Button edit = outlineButton("Edit");
        edit.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) { showLocationDialog(record); }
        });
        row.addView(edit);
        Button delete = outlineButton("Delete");
        delete.setTextColor(RED);
        delete.setOnClickListener(new View.OnClickListener() {
            @Override public void onClick(View v) {
                database.deleteLocation(record.id);
                toast("Location deleted");
                showTab(9);
            }
        });
        row.addView(delete);
        return row;
    }

    private LinearLayout legacyAdminRow(AuditDatabase.AdminRecord record) {
        LinearLayout row = card();
        row.addView(label(record.name, 15, TEXT, true));
        row.addView(label((record.parent.length() == 0 ? "No parent" : record.parent) + " | " +
                (record.detail.length() == 0 ? "No detail" : record.detail), 12, MUTED, false));
        return row;
    }

    private TextView masterDataHelp() {
        TextView help = label(
                "How Master Data Setup Works\n\n" +
                        "Master data is the shared setup used across inspections, scheduled visits, work orders, equipment records, reports, and captain access.\n\n" +
                        "Type: Choose what kind of setup record you are adding.\n" +
                        "Name: Enter the short name users will recognize, such as an outlet code, zone name, category, role, or PIN label.\n" +
                        "Parent / Group: Show where the record belongs, such as Ottotree, an outlet, or a workflow.\n" +
                        "Detail: Add a short explanation so users know when this record should be used.\n\n" +
                        "Outlet: Store or branch codes used for visits, inspections, work orders, and equipment.\n" +
                        "Zone: Areas inside an outlet, such as entrance, display zone, counter, server room, or storage area.\n" +
                        "Department: The team that owns a work order. Manage department choices from the Departments tab.\n" +
                        "Checklist Template: A reusable inspection template or checklist group.\n" +
                        "User Role: Responsibility groups such as auditor, outlet captain, technician, manager, or admin.\n" +
                        "Captain PIN: Outlet captain access details for confirmation and follow-up workflows.",
                12, MUTED, false);
        help.setPadding(dp(14), dp(14), dp(14), dp(14));
        help.setBackground(shape(GREEN_LIGHT, Color.rgb(187, 213, 204), dp(7)));
        return help;
    }

    private TextView emptyText(String text) {
        TextView empty = label(text, 14, MUTED, false);
        empty.setGravity(Gravity.CENTER);
        empty.setPadding(0, dp(34), 0, dp(34));
        return empty;
    }

    private LinearLayout flowStep(String number, String text) {
        LinearLayout row = new LinearLayout(this);
        row.setOrientation(LinearLayout.HORIZONTAL);
        row.setGravity(Gravity.CENTER_VERTICAL);
        row.setPadding(0, dp(8), 0, dp(8));
        TextView badge = label(number, 13, Color.WHITE, true);
        badge.setGravity(Gravity.CENTER);
        badge.setBackground(shape(GREEN, Color.TRANSPARENT, dp(14)));
        row.addView(badge, new LinearLayout.LayoutParams(dp(28), dp(28)));
        LinearLayout.LayoutParams textParams = new LinearLayout.LayoutParams(0, -2, 1);
        textParams.setMargins(dp(10), 0, 0, 0);
        row.addView(label(text, 14, TEXT, false), textParams);
        return row;
    }

    private LinearLayout rankingRow(int rank, AuditDatabase.OutletSummary o, int medalColor) {
        LinearLayout row = auditRow(o.code, o.branch, o.date, o.latest, o.latest >= 90 ? "Excellent" : "Good");
        TextView medal = label(String.valueOf(rank), 14, Color.WHITE, true);
        medal.setGravity(Gravity.CENTER);
        medal.setBackground(shape(medalColor, Color.TRANSPARENT, dp(14)));
        row.addView(medal, 0, new LinearLayout.LayoutParams(dp(28), dp(28)));
        return row;
    }

    private LinearLayout scoreSummary(String title, String value) {
        LinearLayout c = card();
        c.setBackground(shape(GREEN, Color.TRANSPARENT, dp(7)));
        c.addView(label(title, 12, Color.WHITE, false));
        c.addView(label(value, 26, Color.WHITE, true));
        return c;
    }

    private LinearLayout card() {
        LinearLayout c = new LinearLayout(this);
        c.setOrientation(LinearLayout.VERTICAL);
        c.setPadding(dp(14), dp(12), dp(14), dp(12));
        c.setBackground(shape(Color.WHITE, BORDER, dp(7)));
        return c;
    }

    private void row(LinearLayout parent, View a, View b) {
        LinearLayout r = new LinearLayout(this);
        r.setOrientation(LinearLayout.HORIZONTAL);
        r.addView(a, weight());
        r.addView(b, weight());
        parent.addView(r, margin(-1, -2, 0, 0, 0, dp(12)));
    }

    private TextView label(String text, int sp, int color, boolean bold) {
        TextView v = new TextView(this);
        v.setText(text);
        v.setTextSize(sp);
        v.setTextColor(color);
        v.setIncludeFontPadding(true);
        if (bold) v.setTypeface(Typeface.DEFAULT, Typeface.BOLD);
        return v;
    }

    private TextView formLabel(String text) {
        TextView v = label(text, 12, MUTED, false);
        v.setPadding(0, dp(10), 0, dp(4));
        return v;
    }

    private Button action(String text, int color) {
        Button b = new Button(this);
        b.setText(text);
        b.setAllCaps(false);
        b.setTextColor(Color.WHITE);
        b.setTextSize(14);
        b.setMinHeight(0);
        b.setMinimumHeight(0);
        b.setPadding(dp(12), 0, dp(12), 0);
        b.setBackground(shape(color, Color.TRANSPARENT, dp(7)));
        LinearLayout.LayoutParams lp = margin(-2, dp(42), 0, 0, dp(8), 0);
        b.setLayoutParams(lp);
        return b;
    }

    private Button outlineButton(String text) {
        Button b = action(text, Color.WHITE);
        b.setTextColor(Color.rgb(105, 119, 112));
        b.setBackground(shape(Color.WHITE, BORDER, dp(7)));
        return b;
    }

    private EditText input(String hint, boolean centered) {
        EditText e = new EditText(this);
        e.setHint(hint);
        e.setTextSize(14);
        e.setSingleLine(false);
        e.setMinHeight(dp(42));
        e.setPadding(dp(12), 0, dp(12), 0);
        e.setGravity(centered ? Gravity.CENTER : Gravity.CENTER_VERTICAL);
        e.setBackground(shape(Color.WHITE, BORDER, dp(7)));
        return e;
    }

    private Spinner spinner(String[] items) {
        Spinner s = new Spinner(this);
        ArrayAdapter<String> adapter = new ArrayAdapter<String>(this, android.R.layout.simple_spinner_dropdown_item, Arrays.asList(items));
        s.setAdapter(adapter);
        s.setBackground(shape(Color.WHITE, BORDER, dp(7)));
        s.setPadding(dp(8), 0, dp(8), 0);
        s.setMinimumHeight(dp(42));
        return s;
    }

    private LinearLayout.LayoutParams weight() {
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(0, -2, 1);
        lp.setMargins(0, 0, dp(8), dp(8));
        return lp;
    }

    private LinearLayout.LayoutParams margin(int w, int h, int l, int t, int r, int b) {
        LinearLayout.LayoutParams lp = new LinearLayout.LayoutParams(w, h);
        lp.setMargins(l, t, r, b);
        return lp;
    }

    private GradientDrawable shape(int fill, int stroke, int radius) {
        GradientDrawable g = new GradientDrawable();
        g.setColor(fill);
        g.setCornerRadius(radius);
        if (stroke != Color.TRANSPARENT) g.setStroke(1, stroke);
        return g;
    }

    private int dp(int v) {
        return (int) (v * getResources().getDisplayMetrics().density + 0.5f);
    }

    private void toast(String message) {
        Toast.makeText(this, message, Toast.LENGTH_SHORT).show();
    }

    private String rating(int score) {
        if (score >= 90) return "Excellent";
        if (score >= 70) return "Good";
        if (score >= 60) return "Below Expectation";
        return "Critical";
    }

    private String valueOrDefault(EditText input, String fallback) {
        String value = input.getText().toString().trim();
        return value.length() == 0 ? fallback : value;
    }

    private boolean isInvalidSetupSelection(Spinner spinner, String label) {
        String selected = spinner.getSelectedItem() == null ? "" : spinner.getSelectedItem().toString();
        if (selected.startsWith("Select") || selected.startsWith("--") || selected.startsWith("No ")) {
            toast("Set up " + label + " first");
            return true;
        }
        return false;
    }

    private void setSpinnerValue(Spinner spinner, String value) {
        for (int i = 0; i < spinner.getCount(); i++) {
            if (String.valueOf(spinner.getItemAtPosition(i)).equals(value)) {
                spinner.setSelection(i);
                return;
            }
        }
    }

    private String roleKey(String label) {
        if ("Executive Admin".equals(label)) return "executive-admin";
        if ("Manager".equals(label)) return "manager";
        if ("Director".equals(label)) return "director";
        return "admin";
    }

    private String roleLabel(String key) {
        if ("executive-admin".equals(key)) return "Executive Admin";
        if ("manager".equals(key)) return "Manager";
        if ("director".equals(key)) return "Director";
        return "Admin";
    }

    private String[] outletOptions(String unit) {
        return setupOptions("Outlet", "No outlets set up");
    }

    private String[] outletOptions(String unit, boolean includePlaceholder) {
        String[] outlets = outletOptions(unit);
        if (!includePlaceholder) return outlets;
        String[] options = new String[outlets.length + 1];
        options[0] = "Select outlet";
        System.arraycopy(outlets, 0, options, 1, outlets.length);
        return options;
    }

    private String[] departmentOptions() {
        return setupOptions("Department", "No departments set up");
    }

    private String firstDepartment() {
        String[] departments = departmentOptions();
        return departments.length == 0 ? "" : departments[0];
    }

    private String[] locationOptions(String outlet) {
        List<AuditDatabase.LocationRecord> rows = database.getLocations(outlet);
        if (rows.size() == 0) {
            return new String[] { "No locations set up" };
        }
        String[] options = new String[rows.size()];
        for (int i = 0; i < rows.size(); i++) {
            options[i] = rows.get(i).name;
        }
        return options;
    }

    private String[] equipmentNames(List<AuditDatabase.EquipmentRecord> rows) {
        List<String> names = new ArrayList<String>();
        for (int i = 0; i < rows.size(); i++) {
            String name = rows.get(i).name;
            if (!names.contains(name)) names.add(name);
        }
        return names.toArray(new String[names.size()]);
    }

    private AuditDatabase.EquipmentRecord equipmentTemplate(List<AuditDatabase.EquipmentRecord> rows, String name) {
        for (int i = 0; i < rows.size(); i++) {
            if (rows.get(i).name.equals(name)) return rows.get(i);
        }
        return null;
    }

    private String[] inspectionCriteria(AuditDatabase.EquipmentRecord row) {
        String text = row.inspectionCriteria == null || row.inspectionCriteria.length() == 0
                ? AuditDatabase.defaultInspectionCriteria()
                : row.inspectionCriteria;
        String[] raw = text.split("\\r?\\n");
        List<String> values = new ArrayList<String>();
        for (int i = 0; i < raw.length; i++) {
            String value = raw[i].trim();
            if (value.length() > 0) values.add(value);
        }
        if (values.size() == 0) return AuditDatabase.defaultInspectionCriteria().split("\\r?\\n");
        return values.toArray(new String[values.size()]);
    }

    private String[] withAll(String label, String[] values) {
        String[] options = new String[values.length + 1];
        options[0] = label;
        System.arraycopy(values, 0, options, 1, values.length);
        return options;
    }

    private String allValue(Spinner spinner, String allLabel) {
        String value = spinner.getSelectedItem() == null ? "" : spinner.getSelectedItem().toString();
        return allLabel.equals(value) ? "" : value;
    }

    private String[] locationFilterOptions(List<AuditDatabase.EquipmentRecord> rows, String outlet) {
        List<String> values = new ArrayList<String>();
        for (int i = 0; i < rows.size(); i++) {
            AuditDatabase.EquipmentRecord row = rows.get(i);
            if (outlet.length() > 0 && !outlet.equals(row.outlet)) continue;
            if (row.location.length() > 0 && !values.contains(row.location)) values.add(row.location);
        }
        return values.toArray(new String[values.size()]);
    }

    private String[] workOrderLocationOptions(List<AuditDatabase.WorkOrderRecord> rows, String outlet) {
        List<String> values = new ArrayList<String>();
        for (int i = 0; i < rows.size(); i++) {
            AuditDatabase.WorkOrderRecord row = rows.get(i);
            if (outlet.length() > 0 && !outlet.equals(row.outlet)) continue;
            if (row.zone.length() > 0 && !values.contains(row.zone)) values.add(row.zone);
        }
        return values.toArray(new String[values.size()]);
    }

    private String[] uniqueEquipmentValues(List<AuditDatabase.EquipmentRecord> rows, String field) {
        List<String> values = new ArrayList<String>();
        for (int i = 0; i < rows.size(); i++) {
            AuditDatabase.EquipmentRecord row = rows.get(i);
            String value = "brand".equals(field) ? row.brand : row.type;
            if (value.length() > 0 && !values.contains(value)) values.add(value);
        }
        return values.toArray(new String[values.size()]);
    }

    private List<AuditDatabase.EquipmentRecord> filterEquipment(List<AuditDatabase.EquipmentRecord> rows) {
        List<AuditDatabase.EquipmentRecord> filtered = new ArrayList<AuditDatabase.EquipmentRecord>();
        String search = equipmentSearch.toLowerCase();
        for (int i = 0; i < rows.size(); i++) {
            AuditDatabase.EquipmentRecord row = rows.get(i);
            String haystack = (row.name + " " + row.description + " " + row.type + " " + row.operationalStatus + " " +
                    row.code + " " + row.model + " " + row.serialNumber + " " + row.brand + " " + row.outlet + " " + row.location).toLowerCase();
            if (search.length() > 0 && !haystack.contains(search)) continue;
            if (equipmentFilterOutlet.length() > 0 && !equipmentFilterOutlet.equals(row.outlet)) continue;
            if (equipmentFilterLocation.length() > 0 && !equipmentFilterLocation.equals(row.location)) continue;
            if (equipmentFilterType.length() > 0 && !equipmentFilterType.equals(row.type)) continue;
            if (equipmentFilterBrand.length() > 0 && !equipmentFilterBrand.equals(row.brand)) continue;
            filtered.add(row);
        }
        return filtered;
    }

    private List<AuditDatabase.WorkOrderRecord> filterWorkOrders(List<AuditDatabase.WorkOrderRecord> rows) {
        List<AuditDatabase.WorkOrderRecord> filtered = new ArrayList<AuditDatabase.WorkOrderRecord>();
        String search = workOrderSearch.toLowerCase();
        for (int i = 0; i < rows.size(); i++) {
            AuditDatabase.WorkOrderRecord row = rows.get(i);
            String haystack = (row.outlet + " " + row.zone + " " + row.requestType + " " + row.priority + " " +
                    row.title + " " + row.description + " " + row.assignee + " " + row.status).toLowerCase();
            if (search.length() > 0 && !haystack.contains(search)) continue;
            if (workOrderFilterOutlet.length() > 0 && !workOrderFilterOutlet.equals(row.outlet)) continue;
            if (workOrderFilterLocation.length() > 0 && !workOrderFilterLocation.equals(row.zone)) continue;
            if (workOrderFilterDepartment.length() > 0 && !workOrderFilterDepartment.equals(row.requestType)) continue;
            if (workOrderFilterPriority.length() > 0 && !workOrderFilterPriority.equals(row.priority)) continue;
            if (workOrderFilterStatus.length() > 0 && !workOrderFilterStatus.equals(row.status)) continue;
            filtered.add(row);
        }
        return filtered;
    }

    private List<AuditDatabase.UserRecord> filterUsers(List<AuditDatabase.UserRecord> rows) {
        List<AuditDatabase.UserRecord> filtered = new ArrayList<AuditDatabase.UserRecord>();
        String search = userSearch.toLowerCase();
        for (int i = 0; i < rows.size(); i++) {
            AuditDatabase.UserRecord row = rows.get(i);
            String haystack = (row.name + " " + row.email + " " + row.role + " " + row.department + " " +
                    row.title + " " + row.responsibilities).toLowerCase();
            if (search.length() > 0 && !haystack.contains(search)) continue;
            if (userFilterRole.length() > 0 && !userFilterRole.equals(row.role)) continue;
            if (userFilterDepartment.length() > 0 && !userFilterDepartment.equals(row.department)) continue;
            filtered.add(row);
        }
        return filtered;
    }

    private boolean matchesAdminSearch(AuditDatabase.AdminRecord row, String searchValue) {
        String search = searchValue.toLowerCase();
        if (search.length() == 0) return true;
        return (row.name + " " + row.parent + " " + row.detail).toLowerCase().contains(search);
    }

    private String[] setupOptions(String recordType, String emptyLabel) {
        List<String> rows = database.getSetupNames(recordType);
        if (rows.size() == 0) {
            return new String[] { emptyLabel };
        }
        String[] options = new String[rows.size()];
        for (int i = 0; i < rows.size(); i++) {
            options[i] = rows.get(i);
        }
        return options;
    }

    private int scoreFrom(EditText input) {
        String value = input.getText().toString().trim();
        if (value.length() == 0) return 0;
        try {
            int score = Integer.parseInt(value);
            if (score < 0) return 0;
            if (score > 100) return 100;
            return score;
        } catch (NumberFormatException e) {
            return 0;
        }
    }

    private void sortByLatestDesc(List<AuditDatabase.OutletSummary> rows) {
        for (int i = 0; i < rows.size(); i++) {
            for (int j = i + 1; j < rows.size(); j++) {
                if (rows.get(j).latest > rows.get(i).latest) {
                    AuditDatabase.OutletSummary tmp = rows.get(i);
                    rows.set(i, rows.get(j));
                    rows.set(j, tmp);
                }
            }
        }
    }

    public static final class SpaceView extends View {
        public SpaceView(android.content.Context c) { super(c); }
    }

    public final class BarChartView extends View {
        private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        public BarChartView(android.content.Context c) { super(c); }
        @Override protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            int w = getWidth();
            int h = getHeight();
            List<AuditDatabase.OutletSummary> rows = database.getOutletSummaries(activeUnit);
            int count = Math.min(rows.size(), 6);
            if (count == 0) {
                String[] emptyLabels = outletOptions(activeUnit);
                count = Math.min(emptyLabels.length, 6);
            }
            int top = dp(22);
            int left = dp(48);
            int rowH = (h - dp(38)) / Math.max(1, count);
            paint.setTextSize(dp(12));
            for (int i = 0; i < count; i++) {
                String labelText = rows.size() > i ? rows.get(i).code : outletOptions(activeUnit)[i];
                int value = rows.size() > i ? rows.get(i).latest : 0;
                int y = top + i * rowH + rowH / 2;
                paint.setColor(MUTED);
                paint.setStyle(Paint.Style.FILL);
                canvas.drawText(labelText, dp(4), y + dp(5), paint);
                int color = value >= 90 ? Color.rgb(36, 195, 98) : BLUE;
                paint.setColor(color);
                RectF bar = new RectF(left, y - dp(18), left + (w - left - dp(18)) * value / 100f, y + dp(18));
                canvas.drawRoundRect(bar, dp(6), dp(6), paint);
                paint.setColor(Color.WHITE);
                paint.setTypeface(Typeface.DEFAULT_BOLD);
                canvas.drawText(String.valueOf(value), bar.right - dp(30), y + dp(5), paint);
                paint.setTypeface(Typeface.DEFAULT);
            }
        }
    }

    public final class DonutChartView extends View {
        private final Paint paint = new Paint(Paint.ANTI_ALIAS_FLAG);
        public DonutChartView(android.content.Context c) { super(c); }
        @Override protected void onDraw(Canvas canvas) {
            super.onDraw(canvas);
            int size = Math.min(getWidth(), getHeight()) - dp(40);
            int left = (getWidth() - size) / 2;
            int top = dp(24);
            RectF oval = new RectF(left, top, left + size, top + size);
            int[] colors = {Color.rgb(36, 195, 98), BLUE, ORANGE, RED};
            float[] sweeps = {52f, 24f, 10f, 14f};
            float start = -90f;
            paint.setStyle(Paint.Style.STROKE);
            paint.setStrokeWidth(dp(42));
            paint.setStrokeCap(Paint.Cap.BUTT);
            for (int i = 0; i < colors.length; i++) {
                paint.setColor(colors[i]);
                canvas.drawArc(oval, start, sweeps[i] * 3.6f, false, paint);
                start += sweeps[i] * 3.6f;
            }
            paint.setStyle(Paint.Style.FILL);
            paint.setColor(TEXT);
            paint.setTextSize(dp(13));
            paint.setTextAlign(Paint.Align.CENTER);
            canvas.drawText("Excellent  Good  Below Expectation  Critical", getWidth() / 2f, dp(18), paint);
        }
    }
}
