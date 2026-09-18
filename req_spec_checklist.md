# Ottotree Audit Requirements Checklist

Source: `/home/user/Downloads/codex/req_spec.md`

Status legend:
- `[x]` Complete enough for current prototype
- `[~]` Partially implemented
- `[ ]` Not implemented yet

## 1. App Objective

- [~] Conduct facilities audits using mobile devices
- [~] Select audit locations and areas
- [~] Take or upload photos
- [~] Mark defects/issues directly on photos
- [~] Select predefined description/audit categories
- [~] Identify priority issues
- [~] Assign issues to relevant department
- [~] Add comments and remarks
- [~] Submit and store audit records
- [~] Track audit findings and corrective actions
- [~] Generate audit reports
- [~] Calculate audit scores automatically
- [~] Generate charts and management dashboards
- [~] Add signatures to completed audit reports

## 2. User Login

- [x] Login page
- [~] Username and password login
- [x] Forgot password (administrator-reviewed reset request)
- [x] Remember me
- [x] Change password
- [x] Logout
- [~] Register account page

## 3. User Management

- [~] Create users
- [~] Edit users
- [~] Delete users
- [~] Deactivate users
- [~] Reset passwords
- [~] Change user roles
- [~] Assign users to departments
- [~] View user login activity
- [~] Enforce role-based access
- [x] Administrator role
- [~] Auditor role
- [~] Department/PIC role
- [~] Management role
- [~] Create/edit/delete custom roles
- [~] Configure role access by app section

## 4. Main Dashboard

- [x] Total audits
- [x] Audits completed
- [x] Audits pending
- [x] Priority issues
- [x] Non-priority issues
- [x] Outstanding issues
- [x] Completed corrective actions
- [x] Overall audit score
- [x] Audit score bar chart
- [x] Priority vs non-priority chart
- [x] Issues by department chart
- [x] Issues by area chart
- [x] Issues by description category chart
- [x] Monthly audit trend chart

## 5. Create New Audit

- [x] `+ New Audit` primary flow
- [x] Automatic audit reference number, e.g. `AUD-2026-0001`
- [x] Audit date
- [x] Audit time
- [x] Auditor name
- [x] Location/outlet
- [x] Audit type setup
- [x] Remarks

## 6. Area / Location Selection

- [~] Outlet setup
- [~] Location setup
- [~] Zone setup
- [~] Floor setup
- [~] Area setup
- [~] Room hierarchy: outlet -> floor/area -> room
- [~] Add location
- [~] Edit location
- [~] Delete location
- [~] Add room/location equivalent
- [~] Edit room/location equivalent
- [~] Delete room/location equivalent
- [~] Create areas/zones
- [~] Change display order of locations

## 7. Photo / Camera Function

- [~] Open camera
- [~] Upload photo from device/gallery
- [~] Retake photo
- [~] Delete photo
- [~] Add multiple photos
- [~] Store actual photo files, not only filenames

## 8. Photo Marking / Circle Function

- [~] Circle tool
- [~] Arrow tool
- [~] Rectangle tool
- [~] Freehand drawing
- [~] Text annotation
- [~] Undo
- [~] Redo
- [~] Clear
- [~] Save marked photo with finding

## 9. Description / Audit Category

- [~] Category master data
- [x] Default categories from spec
- [x] Add category
- [x] Edit category
- [x] Delete category
- [x] Change category sequence
- [~] Use categories in findings/work orders

## 10. Priority / Non-Priority

- [~] Priority levels in work orders
- [~] Configurable priority wording
- [~] Priority vs non-priority classification
- [~] Due days by priority

## 11. Assign To Department

- [~] Department setup
- [~] Assign issue/work order to department
- [~] Add PIC under department
- [~] Assign findings to specific PIC
- [~] Department/PIC can view assigned findings

## 12. Comment / Remarks

- [~] Free-text remarks on scheduled work
- [~] Free-text notes on failed criteria
- [~] Work order description/comments
- [~] Separate cause/recommendation/required action fields

## 13. Audit Finding Record

- [~] Finding ID, e.g. `F-2026-00125`
- [~] Audit reference number
- [~] Date
- [~] Time
- [~] Auditor
- [~] Location
- [~] Room/area/location
- [~] Photo filename reference
- [~] Stored original photo
- [~] Stored marked photo
- [~] Description category
- [~] Priority
- [~] Assigned department
- [~] PIC
- [~] Comment
- [~] Status
- [~] Corrective action/work order equivalent
- [~] Completion date
- [~] Completion photo
- [~] Completion remark
- [~] Verification status/details

## 14. Submit Function

- [~] Validate completed inspections
- [~] Generate finding ID
- [~] Save finding/work order
- [~] Store photo file
- [~] Store marked photo
- [~] Record auditor
- [~] Record date
- [~] Record time
- [~] Assign to department
- [~] Update dashboard
- [~] Calculate audit score

## 15. Audit Status

- [~] Assigned
- [~] In Progress
- [~] Pending
- [~] Completed
- [~] Verified
- [~] Open
- [~] Closed
- [~] Status transitions/rules

## 16. Corrective Action

- [~] Work order records
- [~] Action taken field
- [~] Person in charge field
- [~] Completion date
- [~] Completion remark
- [~] Completion photo
- [~] Auditor/facilities manager verification workflow

## 17. Audit Scoring System

- [x] Score calculated from inspection criteria
- [x] Total audit item count in report
- [x] Passed count
- [x] Failed count
- [x] Rating bands from spec
- [x] Configurable scoring rules
- [x] Configurable pass mark
- [x] Configurable weighting

## 18. Reporting

- [~] PDF audit report generation
- [~] Professional report layout
- [~] Company logo
- [~] Company name
- [~] Facilities department header
- [~] Location
- [~] Audit date
- [~] Auditor
- [~] Audit reference number

## 19. Report Content

- [~] Audit summary
- [~] Overall audit score
- [~] Rating text
- [~] Total findings
- [~] Priority findings count
- [~] Non-priority findings count
- [~] Completed count
- [~] Outstanding count

## 20. Bar Chart / Graph

- [x] Audit score chart
- [x] Previous vs current audit chart
- [x] Findings by category chart
- [x] Findings by priority chart
- [x] Findings by department chart

## 21. Photo Findings Report

- [~] Finding/checklist rows in PDF
- [~] Location in finding row
- [~] Category in finding row
- [~] Priority via work order
- [~] Assigned department via work order
- [~] Description/comment
- [~] Original photo in report
- [~] Marked photo in report
- [~] Status
- [~] Completion photo in report

## 22. Signature

- [~] Audited by signature
- [~] Verified by signature
- [~] Acknowledged by signature
- [~] Draw signature on screen
- [~] Upload signature
- [~] Store signature in user profile

## 23. Backend Administration

- [~] Users
- [~] User password/status
- [~] Locations
- [~] Floor
- [~] Area/zone
- [~] Room/location
- [~] Description/category setup
- [~] Priority values in forms
- [~] Department setup
- [~] Audit type setup
- [~] Scoring setup
- [~] Report setup
- [~] System settings

## 24. Search & Filter

- [~] Search inspection history
- [~] Search/filter work orders
- [~] Search/filter equipment
- [~] Search/filter users
- [~] Search/filter departments/outlets
- [~] Filter historical audits by date
- [~] Filter historical audits by location/room
- [~] Filter historical audits by auditor
- [~] Filter historical audits by department
- [~] Filter historical audits by category
- [~] Filter historical audits by priority
- [~] Filter historical audits by status
- [~] Filter historical audits by PIC

## 25. Audit History

- [~] Inspection history list
- [~] Continue saved draft inspection
- [~] Delete inspection history
- [~] Progress status in history
- [x] Room/location-specific historical audit trend
- [~] Findings count per historical audit
- [x] Closed audit state

## 26. Management Dashboard

- [x] Overall audit score
- [x] Total audits
- [x] Total findings
- [x] Priority findings
- [x] Outstanding findings
- [x] Overdue findings
- [x] Completion rate
- [x] Monthly audit score chart
- [x] Findings trend chart
- [x] Priority trend chart
- [x] Department performance chart
- [x] Location performance chart
- [x] Category performance chart

## 27. Overdue Alert

- [~] Due date calculation
- [~] Priority due within 3 days
- [~] Non-priority due within 14 days
- [~] Overdue dashboard count
- [~] Overdue visual alert

## 28. Notification

- [x] Assigned finding notification
- [x] Due soon reminder
- [x] Overdue notification
- [x] Completed notification
- [ ] Email integration (deferred external integration)
- [ ] WhatsApp integration (deferred external integration)
- [ ] Mobile push integration (deferred external integration)

## 29. Export

- [~] PDF export
- [x] Excel export
- [x] CSV export
- [x] JSON export
- [x] Detailed finding list export

## 30. Recommended App Menu

- [~] Home/To-do
- [~] New audit/guided inspection equivalent
- [~] My audits/inspection history
- [~] Findings/work orders equivalent
- [~] Corrective action tab
- [~] Reports
- [~] Notifications
- [~] Profile/users equivalent
- [~] Logout

## 31. Recommended Workflow

- [~] Login
- [~] Select location
- [~] Select room/area
- [~] Take/upload photo
- [~] Mark/circle issue
- [~] Select description/category
- [~] Select priority
- [~] Assign to department
- [~] Add comment
- [~] Submit/save
- [~] Finding/work order created
- [~] Assigned to PIC
- [~] Corrective action
- [~] Upload completion photo
- [~] Auditor verification
- [~] Close finding
- [~] Audit completed
- [~] Calculate score
- [~] Generate report
- [~] Management review/reporting

## 32. Audit Checklist

- [~] Predefined equipment inspection criteria
- [x] PASS/FAIL style criteria through checkbox and remark
- [x] N/A option
- [x] Failed item opens work order request
- [~] Failed item flow includes take photo
- [~] Failed item flow includes mark issue
- [x] Failed item flow includes description category
- [x] Failed item flow includes priority
- [x] Failed item flow includes department assignment
- [x] Failed item flow includes comment

## 33. Recommended System Structure

- [~] Android mobile app
- [~] Web admin portal
- [x] SQLite database
- [x] User accounts storage
- [x] Audit records storage
- [x] Photo file storage
- [x] Marked photo storage
- [x] Findings/work orders storage
- [~] Comments as separate timeline
- [~] Corrective action/work order records
- [~] Audit scores
- [~] Reports
- [~] Signatures
- [~] Audit history

## 34. Future Development

- [x] QR code for every room
- [x] QR code/asset ID for equipment
- [~] Asset audit
- [~] Preventive maintenance integration
- [~] Work order creation
- [~] CMMS integration
- [~] Email notification
- [~] WhatsApp notification
- [~] Mobile push notification
- [~] AI photo defect detection
- [~] AI-generated audit summary
- [~] AI recommendation for corrective action
- [~] Vendor assignment
- [~] SLA tracking
- [~] Cost tracking
- [~] Audit trend analysis

## 35. Final App Concept

- [~] Audit
- [~] Finding as first-class entity
- [~] Assignment
- [~] Corrective action via work orders
- [~] Verification
- [~] Score
- [~] Report
- [~] Management analysis

## Recommended Next Implementation Order

1. Create first-class Audit and Finding references (`AUD-YYYY-NNNN`, `F-YYYY-NNNNN`). Started: completed audits now store audit references, failed criteria create finding records, and work orders have their own references.
2. Add configurable Description/Audit Categories. Started: categories are setup-backed with spec defaults and can be added, edited, deleted, ordered, and selected on work orders.
3. Convert failed checklist criteria into Finding records before Work Orders. Started: failed criteria create finding records on completed inspections, and a Findings tab now exposes searchable/filterable finding records.
4. Add actual photo file storage and photo deletion. Started: guided inspection uploads are stored with image metadata and data URLs in saved inspection records, restored with history, and removable before saving.
5. Add corrective action fields: action taken, PIC, completion date, completion remark, completion photo. Started: work orders now store and edit these fields, completion photos are saved as image metadata/data URLs, and linked findings are synchronized when corrective details change.
6. Add close/verify workflow and status rules. Started: work orders now support Verified and Closed states with verifier, verification date, verification remark, and close date, and linked findings receive the same status details.
7. Improve PDF report to include summary, findings, photos, rating, and signatures. Started: inspection PDFs now include company/facilities heading, score rating, checklist totals, finding totals, priority finding count, completed/outstanding counts, categories, corrective action details, verification details, and original/completion photo labels.
8. Add login and role-based access control.
