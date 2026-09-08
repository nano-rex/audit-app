package com.example.ottotreeaudit;

import android.content.ContentValues;
import android.content.Context;
import android.database.Cursor;
import android.database.sqlite.SQLiteDatabase;
import android.database.sqlite.SQLiteOpenHelper;

import java.util.ArrayList;
import java.util.List;

final class AuditDatabase extends SQLiteOpenHelper {
    private static final String DB_NAME = "ottotree_audit.db";
    private static final int DB_VERSION = 11;
    private static final String[] LOUDSPEAKER_OUTLETS = {"STP", "SBA", "TPG", "AQP", "CCS", "SPK", "BSP", "MYT", "DJM", "KPG", "TSU", "TMA", "PGA", "PSC", "PWS"};

    AuditDatabase(Context context) {
        super(context, DB_NAME, null, DB_VERSION);
    }

    @Override
    public void onCreate(SQLiteDatabase db) {
        db.execSQL("CREATE TABLE audits (" +
                "id INTEGER PRIMARY KEY AUTOINCREMENT," +
                "business_unit TEXT NOT NULL," +
                "outlet TEXT NOT NULL," +
                "branch TEXT NOT NULL," +
                "audit_date TEXT NOT NULL," +
                "auditor TEXT NOT NULL," +
                "audit_type TEXT NOT NULL," +
                "score INTEGER NOT NULL," +
                "created_at INTEGER NOT NULL)");

        db.execSQL("CREATE TABLE schedules (" +
                "id INTEGER PRIMARY KEY AUTOINCREMENT," +
                "business_unit TEXT NOT NULL," +
                "outlet TEXT NOT NULL," +
                "zone TEXT," +
                "scheduled_date TEXT NOT NULL," +
                "auditor TEXT NOT NULL," +
                "remarks TEXT," +
                "status TEXT NOT NULL," +
                "created_at INTEGER NOT NULL)");

        db.execSQL("CREATE TABLE captain_logins (" +
                "id INTEGER PRIMARY KEY AUTOINCREMENT," +
                "outlet TEXT NOT NULL," +
                "captain_name TEXT NOT NULL," +
                "logged_at INTEGER NOT NULL)");

        db.execSQL("CREATE TABLE inspection_items (" +
                "id INTEGER PRIMARY KEY AUTOINCREMENT," +
                "audit_id INTEGER NOT NULL," +
                "section TEXT NOT NULL," +
                "item TEXT NOT NULL," +
                "score INTEGER NOT NULL," +
                "notes TEXT," +
                "evidence_status TEXT NOT NULL)");

        db.execSQL("CREATE TABLE work_orders (" +
                "id INTEGER PRIMARY KEY AUTOINCREMENT," +
                "business_unit TEXT NOT NULL," +
                "outlet TEXT NOT NULL," +
                "zone TEXT NOT NULL," +
                "request_type TEXT NOT NULL," +
                "priority TEXT NOT NULL," +
                "title TEXT NOT NULL," +
                "description TEXT," +
                "assignee TEXT NOT NULL," +
                "status TEXT NOT NULL," +
                "outlet_confirmed INTEGER NOT NULL DEFAULT 0," +
                "source_audit_id INTEGER," +
                "source_item_id INTEGER," +
                "created_at INTEGER NOT NULL)");

        db.execSQL("CREATE TABLE equipment (" +
                "id INTEGER PRIMARY KEY AUTOINCREMENT," +
                "asset_id TEXT NOT NULL UNIQUE," +
                "qr_code TEXT NOT NULL," +
                "business_unit TEXT NOT NULL," +
                "outlet TEXT NOT NULL," +
                "zone TEXT NOT NULL," +
                "equipment_type TEXT NOT NULL," +
                "health_status TEXT NOT NULL," +
                "last_checked TEXT NOT NULL," +
                "replacement_flag INTEGER NOT NULL DEFAULT 0," +
                "notes TEXT," +
                "name TEXT," +
                "description TEXT," +
                "type TEXT," +
                "operational_status TEXT," +
                "code TEXT," +
                "model TEXT," +
                "serial_number TEXT," +
                "brand TEXT," +
                "location TEXT," +
                "installation_date TEXT," +
                "inspection_criteria TEXT," +
                "created_at INTEGER NOT NULL)");

        db.execSQL("CREATE TABLE locations (" +
                "id INTEGER PRIMARY KEY AUTOINCREMENT," +
                "outlet_code TEXT NOT NULL," +
                "name TEXT NOT NULL," +
                "size TEXT," +
                "created_at INTEGER NOT NULL," +
                "UNIQUE(outlet_code, name))");

        db.execSQL("CREATE TABLE admin_records (" +
                "id INTEGER PRIMARY KEY AUTOINCREMENT," +
                "record_type TEXT NOT NULL," +
                "name TEXT NOT NULL," +
                "parent TEXT," +
                "detail TEXT," +
                "active INTEGER NOT NULL DEFAULT 1," +
                "created_at INTEGER NOT NULL)");

        db.execSQL("CREATE TABLE users (" +
                "id INTEGER PRIMARY KEY AUTOINCREMENT," +
                "name TEXT NOT NULL," +
                "role TEXT NOT NULL," +
                "email TEXT NOT NULL UNIQUE," +
                "department TEXT," +
                "title TEXT," +
                "responsibilities TEXT," +
                "created_at INTEGER NOT NULL)");

        seed(db);
    }

    @Override
    public void onUpgrade(SQLiteDatabase db, int oldVersion, int newVersion) {
        db.execSQL("DROP TABLE IF EXISTS captain_logins");
        db.execSQL("DROP TABLE IF EXISTS locations");
        db.execSQL("DROP TABLE IF EXISTS users");
        db.execSQL("DROP TABLE IF EXISTS admin_records");
        db.execSQL("DROP TABLE IF EXISTS equipment");
        db.execSQL("DROP TABLE IF EXISTS work_orders");
        db.execSQL("DROP TABLE IF EXISTS inspection_items");
        db.execSQL("DROP TABLE IF EXISTS schedules");
        db.execSQL("DROP TABLE IF EXISTS audits");
        onCreate(db);
    }

    long saveAudit(String businessUnit, String outlet, String auditDate, String auditor, String auditType, int score) {
        ContentValues values = new ContentValues();
        values.put("business_unit", businessUnit);
        values.put("outlet", outlet);
        values.put("branch", "LONG");
        values.put("audit_date", auditDate);
        values.put("auditor", auditor);
        values.put("audit_type", auditType);
        values.put("score", score);
        values.put("created_at", System.currentTimeMillis());
        return getWritableDatabase().insert("audits", null, values);
    }

    long saveInspection(String businessUnit, String outlet, String zone, String auditDate, String auditor, List<InspectionItemInput> items) {
        int total = 0;
        for (int i = 0; i < items.size(); i++) {
            total += clampScore(items.get(i).score);
        }
        int average = items.size() == 0 ? 0 : Math.round(total / (float) items.size());

        SQLiteDatabase db = getWritableDatabase();
        long auditId = -1;
        db.beginTransaction();
        try {
            ContentValues audit = new ContentValues();
            audit.put("business_unit", businessUnit);
            audit.put("outlet", outlet);
            audit.put("branch", zone);
            audit.put("audit_date", auditDate);
            audit.put("auditor", auditor);
            audit.put("audit_type", "Inspection");
            audit.put("score", average);
            audit.put("created_at", System.currentTimeMillis());
            auditId = db.insert("audits", null, audit);

            for (int i = 0; i < items.size(); i++) {
                InspectionItemInput input = items.get(i);
                ContentValues row = new ContentValues();
                row.put("audit_id", auditId);
                row.put("section", input.section);
                row.put("item", input.item);
                row.put("score", clampScore(input.score));
                row.put("notes", input.notes);
                row.put("evidence_status", input.evidenceStatus);
                long itemId = db.insert("inspection_items", null, row);
                int score = clampScore(input.score);
                if (score < 70 && !input.workOrderRequested) {
                    saveWorkOrder(db, businessUnit, outlet, zone, score < 60 ? "High" : "Medium", input.item,
                            input.notes.length() == 0 ? "Created from low inspection score" : input.notes,
                            auditId, itemId);
                }
            }
            db.setTransactionSuccessful();
        } finally {
            db.endTransaction();
        }
        return auditId;
    }

    long saveSchedule(String businessUnit, String outlet, String zone, String scheduledDate, String auditor, String remarks) {
        ContentValues values = new ContentValues();
        values.put("business_unit", businessUnit);
        values.put("outlet", outlet);
        values.put("zone", zone);
        values.put("scheduled_date", scheduledDate);
        values.put("auditor", auditor);
        values.put("remarks", remarks);
        values.put("status", "Pending");
        values.put("created_at", System.currentTimeMillis());
        return getWritableDatabase().insert("schedules", null, values);
    }

    int updateSchedule(int id, String outlet, String zone, String scheduledDate, String auditor, String remarks, String status) {
        ContentValues values = new ContentValues();
        values.put("outlet", outlet);
        values.put("zone", zone);
        values.put("scheduled_date", scheduledDate);
        values.put("auditor", auditor);
        values.put("remarks", remarks);
        values.put("status", status);
        return getWritableDatabase().update("schedules", values, "id = ?", new String[] { String.valueOf(id) });
    }

    int deleteSchedule(int id) {
        return getWritableDatabase().delete("schedules", "id = ?", new String[] { String.valueOf(id) });
    }

    long saveManualWorkOrder(String businessUnit, String outlet, String zone, String requestType, String priority, String title, String description, String assignee) {
        ContentValues values = new ContentValues();
        values.put("business_unit", businessUnit);
        values.put("outlet", outlet);
        values.put("zone", zone);
        values.put("request_type", requestType);
        values.put("priority", priority);
        values.put("title", title);
        values.put("description", description);
        values.put("assignee", assignee);
        values.put("status", "Assigned");
        values.put("outlet_confirmed", 0);
        values.put("created_at", System.currentTimeMillis());
        return getWritableDatabase().insert("work_orders", null, values);
    }

    int updateWorkOrder(int id, String outlet, String zone, String requestType, String priority, String title, String description, String assignee, String status) {
        ContentValues values = new ContentValues();
        values.put("outlet", outlet);
        values.put("zone", zone);
        values.put("request_type", requestType);
        values.put("priority", priority);
        values.put("title", title);
        values.put("description", description);
        values.put("assignee", assignee);
        values.put("status", status);
        return getWritableDatabase().update("work_orders", values, "id = ?", new String[] { String.valueOf(id) });
    }

    int deleteWorkOrder(int id) {
        return getWritableDatabase().delete("work_orders", "id = ?", new String[] { String.valueOf(id) });
    }

    long saveCaptainLogin(String outlet, String captainName) {
        ContentValues values = new ContentValues();
        values.put("outlet", outlet);
        values.put("captain_name", captainName);
        values.put("logged_at", System.currentTimeMillis());
        return getWritableDatabase().insert("captain_logins", null, values);
    }

    void seedSchedulesIfEmpty() {
        SQLiteDatabase db = getWritableDatabase();
        Cursor c = db.rawQuery("SELECT COUNT(*) FROM schedules", null);
        try {
            if (c.moveToFirst() && c.getInt(0) == 0) {
                seedSchedule(db, "Mini Studio", "MST", "Server Room", "2026-09-01", "Ah Fai", "Room checklist before peak hours");
                seedSchedule(db, "Mini Studio", "MQS", "Entrance", "2026-09-02", "Ah Fan", "Photo evidence follow-up");
                seedSchedule(db, "Loudspeaker", "STP", "Display Zone", "2026-09-01", "Gavin", "Speaker display readiness");
            }
        } finally {
            c.close();
        }
    }

    void seedEquipmentIfEmpty() {
        SQLiteDatabase db = getWritableDatabase();
        Cursor c = db.rawQuery("SELECT COUNT(*) FROM equipment", null);
        try {
            if (c.moveToFirst() && c.getInt(0) == 0) {
                seedEquipment(db);
            }
        } finally {
            c.close();
        }
    }

    void seedAdminIfEmpty() {
        SQLiteDatabase db = getWritableDatabase();
        Cursor c = db.rawQuery("SELECT COUNT(*) FROM admin_records", null);
        try {
            if (c.moveToFirst() && c.getInt(0) == 0) {
                seedAdmin(db);
            }
        } finally {
            c.close();
        }
    }

    AuditStats getAuditStats(String businessUnit) {
        String where = scopeWhere("audits", businessUnit);
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT COUNT(*), COALESCE(ROUND(AVG(score)), 0), " +
                        "SUM(CASE WHEN score >= 90 THEN 1 ELSE 0 END), " +
                        "SUM(CASE WHEN score < 60 THEN 1 ELSE 0 END) " +
                        "FROM audits WHERE " + where,
                scopeArgs(businessUnit));
        try {
            if (c.moveToFirst()) {
                return new AuditStats(c.getInt(0), c.getInt(1), c.getInt(2), c.getInt(3));
            }
            return new AuditStats(0, 0, 0, 0);
        } finally {
            c.close();
        }
    }

    void normalizeLoudspeakerOutlets() {
        SQLiteDatabase db = getWritableDatabase();
        String[][] legacyMap = {{"MAM", "STP"}, {"MQS", "SBA"}, {"MDP", "TPG"}, {"MST", "AQP"}};
        String[] tables = {"audits", "schedules", "work_orders", "equipment"};
        for (String[] pair : legacyMap) {
            ContentValues values = new ContentValues();
            values.put("outlet", pair[1]);
            for (String table : tables) {
                db.update(table, values, "business_unit = ? AND outlet = ?", new String[] {"Loudspeaker", pair[0]});
            }
        }
        ContentValues values = new ContentValues();
        values.put("record_type", "Audit Area");
        db.update("admin_records", values, "record_type = ?", new String[] {"Business Unit"});
        db.delete("admin_records", "record_type = ?", new String[] {"Audit Area"});
    }

    void removeSampleAudits() {
        getWritableDatabase().delete("audits", "auditor = ?", new String[] {"Sample Auditor"});
    }

    List<OutletSummary> getOutletSummaries(String businessUnit) {
        String where = scopeWhere("audits", businessUnit);
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT outlet, COALESCE(ROUND(AVG(score)), 0), COUNT(*), " +
                        "(SELECT score FROM audits latest WHERE latest.business_unit = audits.business_unit " +
                        "AND latest.outlet = audits.outlet ORDER BY created_at DESC, id DESC LIMIT 1), " +
                        "(SELECT audit_date FROM audits latest WHERE latest.business_unit = audits.business_unit " +
                        "AND latest.outlet = audits.outlet ORDER BY created_at DESC, id DESC LIMIT 1) " +
                        "FROM audits WHERE " + where + " GROUP BY outlet ORDER BY outlet",
                scopeArgs(businessUnit));
        try {
            List<OutletSummary> rows = new ArrayList<OutletSummary>();
            while (c.moveToNext()) {
                rows.add(new OutletSummary(c.getString(0), "LONG", c.getString(4), c.getInt(1), c.getInt(2), c.getInt(3)));
            }
            return rows;
        } finally {
            c.close();
        }
    }

    List<AuditRecord> getRecentAudits(String businessUnit, int limit) {
        String where = scopeWhere("audits", businessUnit);
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT outlet, branch, audit_date, score FROM audits WHERE " + where + " " +
                        "ORDER BY created_at DESC, id DESC LIMIT " + limit,
                scopeArgs(businessUnit));
        try {
            List<AuditRecord> rows = new ArrayList<AuditRecord>();
            while (c.moveToNext()) {
                rows.add(new AuditRecord(c.getString(0), c.getString(1), c.getString(2), c.getInt(3)));
            }
            return rows;
        } finally {
            c.close();
        }
    }

    KpiStats getKpiStats(String businessUnit) {
        String where = scopeWhere("schedules", businessUnit);
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT COUNT(*), SUM(CASE WHEN status = 'Completed' THEN 1 ELSE 0 END), " +
                        "SUM(CASE WHEN status = 'Pending' THEN 1 ELSE 0 END) FROM schedules WHERE " + where,
                scopeArgs(businessUnit));
        try {
            if (c.moveToFirst()) {
                int assigned = c.getInt(0);
                int completed = c.getInt(1);
                int pending = c.getInt(2);
                int responseRate = assigned == 0 ? 0 : Math.round(completed * 100f / assigned);
                return new KpiStats(assigned, completed, pending, responseRate);
            }
            return new KpiStats(0, 0, 0, 0);
        } finally {
            c.close();
        }
    }

    List<WorkOrderRecord> getWorkOrders(String businessUnit, int limit) {
        String where = scopeWhere("work_orders", businessUnit);
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT id, outlet, zone, request_type, priority, title, description, assignee, status, outlet_confirmed " +
                        "FROM work_orders WHERE " + where + " ORDER BY " +
                        "CASE priority WHEN 'High' THEN 1 WHEN 'Medium' THEN 2 ELSE 3 END, created_at DESC LIMIT " + limit,
                scopeArgs(businessUnit));
        try {
            List<WorkOrderRecord> rows = new ArrayList<WorkOrderRecord>();
            while (c.moveToNext()) {
                rows.add(new WorkOrderRecord(c.getInt(0), c.getString(1), c.getString(2), c.getString(3),
                        c.getString(4), c.getString(5), c.getString(6), c.getString(7), c.getString(8), c.getInt(9) == 1));
            }
            return rows;
        } finally {
            c.close();
        }
    }

    long saveEquipment(String businessUnit, String outlet, String location, String name, String description,
                       String type, String operationalStatus, String code, String model, String serialNumber,
                       String brand, String installationDate, String inspectionCriteria) {
        ContentValues values = new ContentValues();
        putEquipmentValues(values, businessUnit, outlet, location, name, description, type, operationalStatus,
                code, model, serialNumber, brand, installationDate, inspectionCriteria);
        values.put("created_at", System.currentTimeMillis());
        return getWritableDatabase().insertWithOnConflict("equipment", null, values, SQLiteDatabase.CONFLICT_REPLACE);
    }

    int updateEquipment(int id, String businessUnit, String outlet, String location, String name, String description,
                        String type, String operationalStatus, String code, String model, String serialNumber,
                        String brand, String installationDate, String inspectionCriteria) {
        ContentValues values = new ContentValues();
        putEquipmentValues(values, businessUnit, outlet, location, name, description, type, operationalStatus,
                code, model, serialNumber, brand, installationDate, inspectionCriteria);
        return getWritableDatabase().update("equipment", values, "id = ?", new String[] { String.valueOf(id) });
    }

    int deleteEquipment(int id) {
        return getWritableDatabase().delete("equipment", "id = ?", new String[] { String.valueOf(id) });
    }

    int assignEquipmentLocation(int id, String outlet, String location) {
        ContentValues values = new ContentValues();
        values.put("outlet", outlet);
        values.put("location", location);
        values.put("zone", location);
        return getWritableDatabase().update("equipment", values, "id = ?", new String[] { String.valueOf(id) });
    }

    static String defaultInspectionCriteria() {
        return "Present and correctly placed\nClean and free from visible damage\nOperational during inspection\nLabel, cable, or accessory is complete";
    }

    private void putEquipmentValues(ContentValues values, String businessUnit, String outlet, String location,
                                    String name, String description, String type, String operationalStatus,
                                    String code, String model, String serialNumber, String brand,
                                    String installationDate, String inspectionCriteria) {
        values.put("asset_id", code);
        values.put("qr_code", code);
        values.put("business_unit", businessUnit);
        values.put("outlet", outlet);
        values.put("zone", location);
        values.put("equipment_type", type);
        values.put("health_status", operationalStatus);
        values.put("last_checked", installationDate);
        values.put("replacement_flag", "Replace".equals(operationalStatus) ? 1 : 0);
        values.put("notes", description);
        values.put("name", name);
        values.put("description", description);
        values.put("type", type);
        values.put("operational_status", operationalStatus);
        values.put("code", code);
        values.put("model", model);
        values.put("serial_number", serialNumber);
        values.put("brand", brand);
        values.put("location", location);
        values.put("installation_date", installationDate);
        values.put("inspection_criteria", inspectionCriteria == null || inspectionCriteria.length() == 0 ? defaultInspectionCriteria() : inspectionCriteria);
    }

    List<EquipmentRecord> getEquipment(String businessUnit, int limit) {
        String where = scopeWhere("equipment", businessUnit);
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT id, asset_id, qr_code, outlet, zone, equipment_type, health_status, last_checked, replacement_flag, notes, " +
                        "name, description, type, operational_status, code, model, serial_number, brand, location, installation_date, inspection_criteria " +
                        "FROM equipment WHERE " + where + " ORDER BY " +
                        "CASE health_status WHEN 'Replace' THEN 1 WHEN 'Monitor' THEN 2 ELSE 3 END, last_checked DESC LIMIT " + limit,
                scopeArgs(businessUnit));
        try {
            List<EquipmentRecord> rows = new ArrayList<EquipmentRecord>();
            while (c.moveToNext()) {
                rows.add(new EquipmentRecord(c.getInt(0), c.getString(1), c.getString(2), c.getString(3), c.getString(4),
                        c.getString(5), c.getString(6), c.getString(7), c.getInt(8) == 1, c.getString(9),
                        c.getString(10), c.getString(11), c.getString(12), c.getString(13), c.getString(14),
                        c.getString(15), c.getString(16), c.getString(17), c.getString(18), c.getString(19), c.getString(20)));
            }
            return rows;
        } finally {
            c.close();
        }
    }

    List<EquipmentRecord> getEquipmentForOutlet(String outlet) {
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT id, asset_id, qr_code, outlet, zone, equipment_type, health_status, last_checked, replacement_flag, notes, " +
                        "name, description, type, operational_status, code, model, serial_number, brand, location, installation_date, inspection_criteria " +
                        "FROM equipment WHERE outlet = ? ORDER BY COALESCE(name, asset_id), id",
                new String[] { outlet });
        try {
            List<EquipmentRecord> rows = new ArrayList<EquipmentRecord>();
            while (c.moveToNext()) {
                rows.add(new EquipmentRecord(c.getInt(0), c.getString(1), c.getString(2), c.getString(3), c.getString(4),
                        c.getString(5), c.getString(6), c.getString(7), c.getInt(8) == 1, c.getString(9),
                        c.getString(10), c.getString(11), c.getString(12), c.getString(13), c.getString(14),
                        c.getString(15), c.getString(16), c.getString(17), c.getString(18), c.getString(19), c.getString(20)));
            }
            return rows;
        } finally {
            c.close();
        }
    }

    long saveAdminRecord(String recordType, String name, String parent, String detail) {
        ContentValues values = new ContentValues();
        values.put("record_type", recordType);
        values.put("name", name);
        values.put("parent", parent);
        values.put("detail", detail);
        values.put("active", 1);
        values.put("created_at", System.currentTimeMillis());
        return getWritableDatabase().insert("admin_records", null, values);
    }

    int updateAdminRecord(int id, String recordType, String name, String parent, String detail) {
        ContentValues values = new ContentValues();
        values.put("record_type", recordType);
        values.put("name", name);
        values.put("parent", parent);
        values.put("detail", detail);
        return getWritableDatabase().update("admin_records", values, "id = ?", new String[] { String.valueOf(id) });
    }

    int deleteAdminRecord(int id) {
        return getWritableDatabase().delete("admin_records", "id = ?", new String[] { String.valueOf(id) });
    }

    long saveLocation(String outlet, String name, String size) {
        ContentValues values = new ContentValues();
        values.put("outlet_code", outlet);
        values.put("name", name);
        values.put("size", size);
        values.put("created_at", System.currentTimeMillis());
        return getWritableDatabase().insertWithOnConflict("locations", null, values, SQLiteDatabase.CONFLICT_REPLACE);
    }

    int updateLocation(int id, String outlet, String name, String size) {
        ContentValues values = new ContentValues();
        values.put("outlet_code", outlet);
        values.put("name", name);
        values.put("size", size);
        return getWritableDatabase().update("locations", values, "id = ?", new String[] { String.valueOf(id) });
    }

    int deleteLocation(int id) {
        return getWritableDatabase().delete("locations", "id = ?", new String[] { String.valueOf(id) });
    }

    List<LocationRecord> getLocations(String outlet) {
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT id, outlet_code, name, size FROM locations WHERE outlet_code = ? ORDER BY name",
                new String[] { outlet });
        try {
            List<LocationRecord> rows = new ArrayList<LocationRecord>();
            while (c.moveToNext()) {
                rows.add(new LocationRecord(c.getInt(0), c.getString(1), c.getString(2), c.getString(3)));
            }
            return rows;
        } finally {
            c.close();
        }
    }

    long saveUser(String name, String role, String email, String department, String title, String responsibilities) {
        ContentValues values = new ContentValues();
        values.put("name", name);
        values.put("role", role);
        values.put("email", email);
        values.put("department", department);
        values.put("title", title);
        values.put("responsibilities", responsibilities);
        values.put("created_at", System.currentTimeMillis());
        return getWritableDatabase().insertWithOnConflict("users", null, values, SQLiteDatabase.CONFLICT_REPLACE);
    }

    int updateUser(int id, String name, String role, String email, String department, String title, String responsibilities) {
        ContentValues values = new ContentValues();
        values.put("name", name);
        values.put("role", role);
        values.put("email", email);
        values.put("department", department);
        values.put("title", title);
        values.put("responsibilities", responsibilities);
        return getWritableDatabase().update("users", values, "id = ?", new String[] { String.valueOf(id) });
    }

    int deleteUser(int id) {
        return getWritableDatabase().delete("users", "id = ?", new String[] { String.valueOf(id) });
    }

    List<UserRecord> getUsers() {
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT id, name, role, email, department, title, responsibilities FROM users ORDER BY role, name",
                null);
        try {
            List<UserRecord> rows = new ArrayList<UserRecord>();
            while (c.moveToNext()) {
                rows.add(new UserRecord(c.getInt(0), c.getString(1), c.getString(2), c.getString(3), c.getString(4), c.getString(5), c.getString(6)));
            }
            return rows;
        } finally {
            c.close();
        }
    }

    List<AdminRecord> getAdminRecords() {
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT id, record_type, name, parent, detail, active FROM admin_records ORDER BY record_type, name",
                null);
        try {
            List<AdminRecord> rows = new ArrayList<AdminRecord>();
            while (c.moveToNext()) {
                rows.add(new AdminRecord(c.getInt(0), c.getString(1), c.getString(2), c.getString(3), c.getString(4), c.getInt(5) == 1));
            }
            return rows;
        } finally {
            c.close();
        }
    }

    List<String> getSetupNames(String recordType) {
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT name FROM admin_records WHERE record_type = ? AND active = 1 ORDER BY name",
                new String[] { recordType });
        try {
            List<String> rows = new ArrayList<String>();
            while (c.moveToNext()) {
                rows.add(c.getString(0));
            }
            return rows;
        } finally {
            c.close();
        }
    }

    private void saveWorkOrder(SQLiteDatabase db, String businessUnit, String outlet, String zone, String priority,
                               String title, String description, long auditId, long itemId) {
        ContentValues values = new ContentValues();
        values.put("business_unit", businessUnit);
        values.put("outlet", outlet);
        values.put("zone", zone);
        values.put("request_type", firstSetupName(db, "Department"));
        values.put("priority", priority);
        values.put("title", title);
        values.put("description", description);
        values.put("assignee", "Technical Support");
        values.put("status", "Assigned");
        values.put("outlet_confirmed", 0);
        values.put("source_audit_id", auditId);
        values.put("source_item_id", itemId);
        values.put("created_at", System.currentTimeMillis());
        db.insert("work_orders", null, values);
    }

    private String firstSetupName(SQLiteDatabase db, String recordType) {
        Cursor c = db.rawQuery(
                "SELECT name FROM admin_records WHERE record_type = ? AND active = 1 ORDER BY name LIMIT 1",
                new String[] { recordType });
        try {
            return c.moveToFirst() ? c.getString(0) : "";
        } finally {
            c.close();
        }
    }

    private void seed(SQLiteDatabase db) {
        seedSchedule(db, "Mini Studio", "MST", "Server Room", "2026-09-01", "Ah Fai", "Room checklist before peak hours");
        seedSchedule(db, "Mini Studio", "MQS", "Entrance", "2026-09-02", "Ah Fan", "Photo evidence follow-up");
        seedSchedule(db, "Loudspeaker", "STP", "Display Zone", "2026-09-01", "Gavin", "Speaker display readiness");
        seedEquipment(db);
        seedAdmin(db);
        seedLocations(db);
        seedUsers(db);
    }

    private void seedAdmin(SQLiteDatabase db) {
        seedAdminRow(db, "Outlet", "MAM", "LONG", "Active outlet");
        seedAdminRow(db, "Outlet", "MQS", "LONG", "Active outlet");
        seedAdminRow(db, "Outlet", "MDP", "LONG", "Active outlet");
        seedAdminRow(db, "Outlet", "MST", "LONG", "Active outlet");
        for (String outlet : LOUDSPEAKER_OUTLETS) {
            seedAdminRow(db, "Outlet", outlet, "Loudspeaker", "Active Loudspeaker outlet");
        }
        seedAdminRow(db, "Zone", "Server Room", "All outlets", "Servers, UPS, network hardware");
        seedAdminRow(db, "Zone", "Entrance", "All outlets", "Front entrance and display area");
        seedAdminRow(db, "Zone", "Display Zone", "All outlets", "Speaker/headphone display fixtures");
        seedAdminRow(db, "Department", "SSD", "Work Orders", "SSD department");
        seedAdminRow(db, "Department", "FMS", "Work Orders", "FMS department");
        seedAdminRow(db, "Department", "AVC", "Work Orders", "AVC department");
        seedAdminRow(db, "Department", "CLD", "Work Orders", "CLD department");
        seedAdminRow(db, "Department", "MD", "Work Orders", "MD department");
        seedAdminRow(db, "Captain PIN", "MST -> 1113", "Captain Login", "Outlet captain access");
        seedAdminRow(db, "Captain PIN", "MAM -> 1213", "Captain Login", "Outlet captain access");
        seedAdminRow(db, "Captain PIN", "MQS -> 1220", "Captain Login", "Outlet captain access");
        seedAdminRow(db, "Captain PIN", "MDP -> 0201", "Captain Login", "Outlet captain access");
    }

    private void seedAdminRow(SQLiteDatabase db, String recordType, String name, String parent, String detail) {
        ContentValues values = new ContentValues();
        values.put("record_type", recordType);
        values.put("name", name);
        values.put("parent", parent);
        values.put("detail", detail);
        values.put("active", 1);
        values.put("created_at", System.currentTimeMillis());
        db.insert("admin_records", null, values);
    }

    private void seedEquipment(SQLiteDatabase db) {
        seedEquipmentRow(db, "EQ-MST-UPS-001", "QR-MST-UPS-001", "Mini Studio", "MST", "Server Room", "UPS", "Monitor", "2026-08-20", true, "Battery age needs budget review");
        seedEquipmentRow(db, "EQ-MQS-CCTV-002", "QR-MQS-CCTV-002", "Mini Studio", "MQS", "Entrance", "CCTV", "Normal", "2026-08-18", false, "Camera view clear");
        seedEquipmentRow(db, "EQ-STP-SPK-001", "QR-STP-SPK-001", "Loudspeaker", "STP", "Display Zone", "Speaker Display", "Normal", "2026-08-22", false, "Demo unit working");
        seedEquipmentRow(db, "EQ-MDP-SRV-001", "QR-MDP-SRV-001", "Mini Studio", "MDP", "Server Room", "Server", "Replace", "2026-08-15", true, "Old hard disk health warning");
    }

    private void seedLocations(SQLiteDatabase db) {
        String[] outlets = {"MAM", "MQS", "MDP", "MST", "STP", "SBA", "TPG", "AQP", "CCS", "SPK", "BSP", "MYT", "DJM", "KPG", "TSU", "TMA", "PGA", "PSC", "PWS"};
        for (String outlet : outlets) {
            seedLocationRow(db, outlet, "R-01", "Room 01");
            seedLocationRow(db, outlet, "R-02", "Room 02");
            seedLocationRow(db, outlet, "R-03", "Room 03");
        }
    }

    private void seedLocationRow(SQLiteDatabase db, String outlet, String name, String size) {
        ContentValues values = new ContentValues();
        values.put("outlet_code", outlet);
        values.put("name", name);
        values.put("size", size);
        values.put("created_at", System.currentTimeMillis());
        db.insertWithOnConflict("locations", null, values, SQLiteDatabase.CONFLICT_IGNORE);
    }

    private void seedUsers(SQLiteDatabase db) {
        seedUserRow(db, "Admin User", "Admin", "admin@ottotree.local", "SSD", "Admin", "Daily operations, inspections, work orders, and setup");
        seedUserRow(db, "Executive Admin", "Executive Admin", "executive.admin@ottotree.local", "FMS", "Executive Admin", "Full workflow oversight, setup, reports, and users");
        seedUserRow(db, "Manager User", "Manager", "manager@ottotree.local", "AVC", "Manager", "Reports, departments, outlets, and user oversight");
        seedUserRow(db, "Director User", "Director", "director@ottotree.local", "MD", "Director", "Reports, departments, outlets, and user oversight");
    }

    private void seedUserRow(SQLiteDatabase db, String name, String role, String email, String department, String title, String responsibilities) {
        ContentValues values = new ContentValues();
        values.put("name", name);
        values.put("role", role);
        values.put("email", email);
        values.put("department", department);
        values.put("title", title);
        values.put("responsibilities", responsibilities);
        values.put("created_at", System.currentTimeMillis());
        db.insertWithOnConflict("users", null, values, SQLiteDatabase.CONFLICT_IGNORE);
    }

    private void seedEquipmentRow(SQLiteDatabase db, String assetId, String qrCode, String businessUnit, String outlet,
                                  String zone, String equipmentType, String healthStatus, String lastChecked,
                                  boolean replacementFlag, String notes) {
        ContentValues values = new ContentValues();
        values.put("asset_id", assetId);
        values.put("qr_code", qrCode);
        values.put("business_unit", businessUnit);
        values.put("outlet", outlet);
        values.put("zone", zone);
        values.put("equipment_type", equipmentType);
        values.put("health_status", healthStatus);
        values.put("last_checked", lastChecked);
        values.put("replacement_flag", replacementFlag ? 1 : 0);
        values.put("notes", notes);
        values.put("name", assetId);
        values.put("description", notes);
        values.put("type", equipmentType);
        values.put("operational_status", healthStatus);
        values.put("code", assetId);
        values.put("model", "");
        values.put("serial_number", "");
        values.put("brand", "");
        values.put("location", zone);
        values.put("installation_date", lastChecked);
        values.put("inspection_criteria", defaultInspectionCriteria());
        values.put("created_at", System.currentTimeMillis());
        db.insertWithOnConflict("equipment", null, values, SQLiteDatabase.CONFLICT_IGNORE);
    }

    private void seedAudit(SQLiteDatabase db, String businessUnit, String outlet, String auditDate, String auditor, String auditType, int score) {
        ContentValues values = new ContentValues();
        values.put("business_unit", businessUnit);
        values.put("outlet", outlet);
        values.put("branch", "LONG");
        values.put("audit_date", auditDate);
        values.put("auditor", auditor);
        values.put("audit_type", auditType);
        values.put("score", score);
        values.put("created_at", System.currentTimeMillis() - (100000L * score));
        db.insert("audits", null, values);
    }

    private void seedSchedule(SQLiteDatabase db, String businessUnit, String outlet, String zone, String scheduledDate, String auditor, String remarks) {
        ContentValues values = new ContentValues();
        values.put("business_unit", businessUnit);
        values.put("outlet", outlet);
        values.put("zone", zone);
        values.put("scheduled_date", scheduledDate);
        values.put("auditor", auditor);
        values.put("remarks", remarks);
        values.put("status", "Pending");
        values.put("created_at", System.currentTimeMillis());
        db.insert("schedules", null, values);
    }

    List<ScheduleRecord> getPendingSchedules(String businessUnit, int limit) {
        String where = scopeWhere("schedules", businessUnit);
        Cursor c = getReadableDatabase().rawQuery(
                "SELECT id, outlet, zone, scheduled_date, auditor, remarks, status FROM schedules WHERE " + where + " " +
                        "ORDER BY scheduled_date ASC, created_at DESC, id DESC LIMIT " + limit,
                scopeArgs(businessUnit));
        try {
            List<ScheduleRecord> rows = new ArrayList<ScheduleRecord>();
            while (c.moveToNext()) {
                rows.add(new ScheduleRecord(c.getInt(0), c.getString(1), c.getString(2), c.getString(3), c.getString(4), c.getString(5), c.getString(6)));
            }
            return rows;
        } finally {
            c.close();
        }
    }

    List<ChecklistItem> getChecklist(String businessUnit) {
        List<ChecklistItem> rows = new ArrayList<ChecklistItem>();
        if (!"Loudspeaker".equals(businessUnit)) {
            rows.add(new ChecklistItem("Main Entrance", "Big headphone display is present and in good condition"));
            rows.add(new ChecklistItem("Studio Area", "Demo headphones are clean, working, and correctly placed"));
            rows.add(new ChecklistItem("Counter", "F&B counter cabinet and cashier drawer area are clean"));
            rows.add(new ChecklistItem("Safety", "Emergency exit and walkway are clear and usable"));
        }
        if (!"Mini Studio".equals(businessUnit)) {
            rows.add(new ChecklistItem("Loudspeaker Display", "Main speaker display is present, clean, and powered"));
            rows.add(new ChecklistItem("Loudspeaker Display", "Price tags and product cards are accurate"));
            rows.add(new ChecklistItem("Loudspeaker Demo", "Demo audio source and cables are working"));
            rows.add(new ChecklistItem("Loudspeaker Safety", "Power socket, cable routing, and fixture are safe"));
        }
        return rows;
    }

    private String scopeWhere(String tableName, String businessUnit) {
        if ("Mini Studio".equals(businessUnit) || "Loudspeaker".equals(businessUnit)) {
            return tableName + ".business_unit = ?";
        }
        return "1 = 1";
    }

    private String[] scopeArgs(String businessUnit) {
        if ("Mini Studio".equals(businessUnit) || "Loudspeaker".equals(businessUnit)) {
            return new String[] { businessUnit };
        }
        return new String[0];
    }

    private int clampScore(int score) {
        if (score < 0) return 0;
        if (score > 100) return 100;
        return score;
    }

    static final class AuditStats {
        final int total;
        final int average;
        final int excellent;
        final int below60;

        AuditStats(int total, int average, int excellent, int below60) {
            this.total = total;
            this.average = average;
            this.excellent = excellent;
            this.below60 = below60;
        }
    }

    static final class OutletSummary {
        final String code;
        final String branch;
        final String date;
        final int average;
        final int auditCount;
        final int latest;

        OutletSummary(String code, String branch, String date, int average, int auditCount, int latest) {
            this.code = code;
            this.branch = branch;
            this.date = date;
            this.average = average;
            this.auditCount = auditCount;
            this.latest = latest;
        }
    }

    static final class AuditRecord {
        final String outlet;
        final String branch;
        final String date;
        final int score;

        AuditRecord(String outlet, String branch, String date, int score) {
            this.outlet = outlet;
            this.branch = branch;
            this.date = date;
            this.score = score;
        }
    }

    static final class KpiStats {
        final int assigned;
        final int completed;
        final int pending;
        final int responseRate;

        KpiStats(int assigned, int completed, int pending, int responseRate) {
            this.assigned = assigned;
            this.completed = completed;
            this.pending = pending;
            this.responseRate = responseRate;
        }
    }

    static final class ScheduleRecord {
        final int id;
        final String outlet;
        final String zone;
        final String scheduledDate;
        final String auditor;
        final String remarks;
        final String status;

        ScheduleRecord(int id, String outlet, String zone, String scheduledDate, String auditor, String remarks, String status) {
            this.id = id;
            this.outlet = outlet;
            this.zone = zone == null || zone.length() == 0 ? "Unassigned" : zone;
            this.scheduledDate = scheduledDate;
            this.auditor = auditor;
            this.remarks = remarks == null ? "" : remarks;
            this.status = status;
        }
    }

    static final class WorkOrderRecord {
        final int id;
        final String outlet;
        final String zone;
        final String requestType;
        final String priority;
        final String title;
        final String description;
        final String assignee;
        final String status;
        final boolean outletConfirmed;

        WorkOrderRecord(int id, String outlet, String zone, String requestType, String priority, String title,
                        String description, String assignee, String status, boolean outletConfirmed) {
            this.id = id;
            this.outlet = outlet;
            this.zone = zone;
            this.requestType = requestType;
            this.priority = priority;
            this.title = title;
            this.description = description == null ? "" : description;
            this.assignee = assignee;
            this.status = status;
            this.outletConfirmed = outletConfirmed;
        }
    }

    static final class EquipmentRecord {
        final int id;
        final String assetId;
        final String qrCode;
        final String outlet;
        final String zone;
        final String equipmentType;
        final String healthStatus;
        final String lastChecked;
        final boolean replacementFlag;
        final String notes;
        final String name;
        final String description;
        final String type;
        final String operationalStatus;
        final String code;
        final String model;
        final String serialNumber;
        final String brand;
        final String location;
        final String installationDate;
        final String inspectionCriteria;

        EquipmentRecord(int id, String assetId, String qrCode, String outlet, String zone, String equipmentType,
                        String healthStatus, String lastChecked, boolean replacementFlag, String notes,
                        String name, String description, String type, String operationalStatus, String code,
                        String model, String serialNumber, String brand, String location, String installationDate,
                        String inspectionCriteria) {
            this.id = id;
            this.assetId = assetId;
            this.qrCode = qrCode;
            this.outlet = outlet;
            this.zone = zone;
            this.equipmentType = equipmentType;
            this.healthStatus = healthStatus;
            this.lastChecked = lastChecked;
            this.replacementFlag = replacementFlag;
            this.notes = notes == null ? "" : notes;
            this.name = name == null || name.length() == 0 ? assetId : name;
            this.description = description == null ? "" : description;
            this.type = type == null || type.length() == 0 ? equipmentType : type;
            this.operationalStatus = operationalStatus == null || operationalStatus.length() == 0 ? healthStatus : operationalStatus;
            this.code = code == null || code.length() == 0 ? assetId : code;
            this.model = model == null ? "" : model;
            this.serialNumber = serialNumber == null ? "" : serialNumber;
            this.brand = brand == null ? "" : brand;
            this.location = location == null || location.length() == 0 ? zone : location;
            this.installationDate = installationDate == null || installationDate.length() == 0 ? lastChecked : installationDate;
            this.inspectionCriteria = inspectionCriteria == null || inspectionCriteria.length() == 0 ? defaultInspectionCriteria() : inspectionCriteria;
        }
    }

    static final class AdminRecord {
        final int id;
        final String recordType;
        final String name;
        final String parent;
        final String detail;
        final boolean active;

        AdminRecord(int id, String recordType, String name, String parent, String detail, boolean active) {
            this.id = id;
            this.recordType = recordType;
            this.name = name;
            this.parent = parent == null ? "" : parent;
            this.detail = detail == null ? "" : detail;
            this.active = active;
        }
    }

    static final class UserRecord {
        final int id;
        final String name;
        final String role;
        final String email;
        final String department;
        final String title;
        final String responsibilities;

        UserRecord(int id, String name, String role, String email, String department, String title, String responsibilities) {
            this.id = id;
            this.name = name;
            this.role = role;
            this.email = email;
            this.department = department == null ? "" : department;
            this.title = title == null ? "" : title;
            this.responsibilities = responsibilities == null ? "" : responsibilities;
        }
    }

    static final class LocationRecord {
        final int id;
        final String outlet;
        final String name;
        final String size;

        LocationRecord(int id, String outlet, String name, String size) {
            this.id = id;
            this.outlet = outlet;
            this.name = name;
            this.size = size == null ? "" : size;
        }
    }

    static final class ChecklistItem {
        final String section;
        final String item;

        ChecklistItem(String section, String item) {
            this.section = section;
            this.item = item;
        }
    }

    static final class InspectionItemInput {
        final String section;
        final String item;
        final int score;
        final String notes;
        final String evidenceStatus;
        final boolean workOrderRequested;

        InspectionItemInput(String section, String item, int score, String notes, String evidenceStatus) {
            this(section, item, score, notes, evidenceStatus, false);
        }

        InspectionItemInput(String section, String item, int score, String notes, String evidenceStatus, boolean workOrderRequested) {
            this.section = section;
            this.item = item;
            this.score = score;
            this.notes = notes;
            this.evidenceStatus = evidenceStatus;
            this.workOrderRequested = workOrderRequested;
        }
    }
}
