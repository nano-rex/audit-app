# Ottotree Audit Requirements Checklist

Source: `/home/user/Downloads/codex/req_spec.md`

Status legend:
- `[x]` Implemented and verified for the stated scope
- `[~]` Partially implemented or verification still incomplete
- `[ ]` Not implemented; deferred integrations are explicitly labeled

Update each item's status when its implementation and verification are complete.
Verification evidence and remaining limitations are recorded in `requirements-progress.md`.

## 1. App Objective

- [x] Conduct facilities audits using mobile devices
- [x] Select audit locations and areas
- [x] Take or upload photos
- [x] Mark defects/issues directly on photos
- [x] Select predefined description/audit categories
- [x] Identify priority issues
- [x] Assign issues to relevant department
- [x] Add comments and remarks
- [x] Submit and store audit records
- [x] Track audit findings and corrective actions
- [x] Generate audit reports
- [x] Calculate audit scores automatically
- [x] Generate charts and management dashboards
- [x] Add signatures to completed audit reports

## 2. User Login

- [x] Login page
- [x] Email/username and password login
- [x] Forgot password (administrator-reviewed reset request)
- [x] Remember me
- [x] Change password
- [x] Logout
- [x] Register account page; new accounts require administrator activation

## 3. User Management

- [x] Create users
- [x] Edit users
- [x] Delete unused users; deactivate accounts with retained login/audit history
- [x] Deactivate users
- [x] Reset passwords
- [x] Change user roles
- [x] Assign users to departments
- [x] View user login activity
- [x] Enforce role-based access
- [x] Administrator role
- [x] Auditor role
- [x] Department/PIC role
- [x] Management role
- [x] Create/edit/delete custom roles
- [x] Configure role access by app section

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

- [x] Outlet setup
- [x] Location setup
- [x] Zone setup
- [x] Floor setup
- [x] Area setup
- [x] Room hierarchy: outlet -> floor/area -> room
- [x] Add location
- [x] Edit location
- [x] Delete location
- [x] Add room/location equivalent
- [x] Edit room/location equivalent
- [x] Delete room/location equivalent
- [x] Create areas/zones
- [x] Change display order of locations

## 7. Photo / Camera Function

- [x] Open camera
- [x] Upload photo from device/gallery
- [x] Retake photo by removing and recapturing/reselecting
- [x] Delete photo
- [x] Add multiple photos
- [x] Store actual photo files, not only filenames

## 8. Photo Marking / Circle Function

- [x] Circle tool
- [x] Arrow tool
- [x] Rectangle tool
- [x] Freehand drawing
- [x] Text annotation
- [x] Undo
- [x] Redo
- [x] Clear
- [x] Save marked photo with finding

## 9. Description / Audit Category

- [x] Category master data
- [x] Default categories from spec
- [x] Add category
- [x] Edit category
- [x] Delete category
- [x] Change category sequence
- [x] Use categories in findings/work orders

## 10. Priority / Non-Priority

- [x] Priority levels in work orders
- [x] Configurable priority wording
- [x] Priority vs non-priority classification
- [x] Due days by priority

## 11. Assign To Department

- [x] Department setup
- [x] Assign issue/work order to department
- [x] Add PIC under department
- [x] Assign findings to specific PIC
- [x] Department/PIC can view assigned findings

## 12. Comment / Remarks

- [x] Free-text remarks on scheduled work
- [x] Free-text notes on failed criteria
- [x] Work order description/comments
- [x] Separate cause/recommendation/required action fields

## 13. Audit Finding Record

- [x] Finding ID, e.g. `F-2026-00125`
- [x] Audit reference number
- [x] Date
- [x] Time
- [x] Auditor
- [x] Location
- [x] Room/area/location
- [x] Photo filename reference
- [x] Stored original photo
- [x] Stored marked photo
- [x] Description category
- [x] Priority
- [x] Assigned department
- [x] PIC
- [x] Comment
- [x] Status
- [x] Corrective action/work order equivalent
- [x] Completion date
- [x] Completion photo
- [x] Completion remark
- [x] Verification status/details

## 14. Submit Function

- [x] Validate completed inspections
- [x] Generate finding ID
- [x] Save finding/work order
- [x] Store photo file
- [x] Store marked photo
- [x] Record auditor
- [x] Record date
- [x] Record time
- [x] Assign to department
- [x] Update dashboard
- [x] Calculate audit score

## 15. Audit Status

- [x] Assigned
- [x] In Progress
- [x] Pending
- [x] Completed
- [x] Verified
- [x] Open
- [x] Closed
- [x] Status transitions/rules

## 16. Corrective Action

- [x] Work order records
- [x] Action taken field
- [x] Person in charge field
- [x] Completion date
- [x] Completion remark
- [x] Completion photo
- [x] Auditor/facilities manager verification workflow

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

- [x] PDF audit report generation
- [x] Professional report layout
- [x] Company logo
- [x] Company name
- [x] Facilities department header
- [x] Location
- [x] Audit date
- [x] Auditor
- [x] Audit reference number

## 19. Report Content

- [x] Audit summary
- [x] Overall audit score
- [x] Rating text
- [x] Total findings
- [x] Priority findings count
- [x] Non-priority findings count
- [x] Completed count
- [x] Outstanding count

## 20. Bar Chart / Graph

- [x] Audit score chart
- [x] Previous vs current audit chart
- [x] Findings by category chart
- [x] Findings by priority chart
- [x] Findings by department chart

## 21. Photo Findings Report

- [x] Finding/checklist rows in PDF
- [x] Location in finding row
- [x] Category in finding row
- [x] Priority via work order
- [x] Assigned department via work order
- [x] Description/comment
- [x] Original photo in report
- [x] Marked photo in report
- [x] Status
- [x] Completion photo in report

## 22. Signature

- [x] Audited by signature
- [x] Verified by signature
- [x] Acknowledged by signature
- [x] Draw signature on screen
- [x] Upload signature
- [x] Store signature in user profile

## 23. Backend Administration

- [x] Users
- [x] User password/status
- [x] Locations
- [x] Floor
- [x] Area/zone
- [x] Room/location
- [x] Description/category setup
- [x] Priority values in forms
- [x] Department setup
- [x] Audit type setup
- [x] Scoring setup
- [x] Report setup
- [x] System settings

## 24. Search & Filter

- [x] Search inspection history
- [x] Search/filter work orders
- [x] Search/filter equipment
- [x] Search/filter users
- [x] Search/filter departments/outlets
- [x] Filter historical audits by date
- [x] Filter historical audits by location/room
- [x] Filter historical audits by auditor
- [x] Filter historical audits by department
- [x] Filter historical audits by category
- [x] Filter historical audits by priority
- [x] Filter historical audits by status
- [x] Filter historical audits by PIC

## 25. Audit History

- [x] Inspection history list
- [x] Continue saved draft inspection
- [x] Delete unfinished inspection drafts; completed records are retained
- [x] Progress status in history
- [x] Room/location-specific historical audit trend
- [x] Findings count per historical audit
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

- [x] Due date calculation
- [x] Priority due within 3 days
- [x] Non-priority due within 14 days
- [x] Overdue dashboard count
- [x] Overdue visual alert

## 28. Notification

- [x] Assigned finding notification
- [x] Due soon reminder
- [x] Overdue notification
- [x] Completed notification
- [ ] Email integration (deferred external integration)
- [ ] WhatsApp integration (deferred external integration)
- [ ] Mobile push integration (deferred external integration)

## 29. Export

- [x] PDF export
- [x] Excel export
- [x] CSV export
- [x] JSON export
- [x] Detailed finding list export

## 30. Recommended App Menu

- [x] Reorderable permitted-page menu, responsive navbar, and per-account saved order

- [x] Home/To-do
- [x] New audit/guided inspection equivalent
- [x] My audits/inspection history
- [x] Findings/work orders equivalent
- [x] Corrective action tab
- [x] Reports
- [x] Notifications
- [x] Profile/users equivalent
- [x] Logout

## 31. Recommended Workflow

- [x] Login
- [x] Select location
- [x] Select room/area
- [x] Take/upload photo
- [x] Mark/circle issue
- [x] Select description/category
- [x] Select priority
- [x] Assign to department
- [x] Add comment
- [x] Submit/save
- [x] Finding/work order created
- [x] Assigned to PIC
- [x] Corrective action
- [x] Upload completion photo
- [x] Auditor verification
- [x] Close finding
- [x] Audit completed
- [x] Calculate score
- [x] Generate report
- [x] Management review/reporting

## 32. Audit Checklist

- [x] Predefined equipment inspection criteria
- [x] PASS/FAIL style criteria through checkbox and remark
- [x] N/A option
- [x] Failed item opens work order request
- [x] Failed item flow includes take photo
- [x] Failed item flow includes mark issue
- [x] Failed item flow includes description category
- [x] Failed item flow includes priority
- [x] Failed item flow includes department assignment
- [x] Failed item flow includes comment

## 33. Recommended System Structure

- [x] Offline Android mobile app; web synchronization remains deferred
- [x] Web admin portal
- [x] SQLite database
- [x] User accounts storage
- [x] Audit records storage
- [x] Photo file storage
- [x] Marked photo storage
- [x] Findings/work orders storage
- [x] Comments as separate timeline
- [x] Corrective action/work order records
- [x] Audit scores
- [x] Reports
- [x] Signatures
- [x] Audit history

## 34. Future Development

- [x] QR code for every room
- [x] QR code/asset ID for equipment
- [x] Asset audit
- [ ] Preventive maintenance integration (deferred future integration)
- [x] Work order creation
- [ ] CMMS integration (deferred future integration)
- [ ] Email notification (deferred future integration)
- [ ] WhatsApp notification (deferred future integration)
- [ ] Mobile push notification (deferred future integration)
- [ ] AI photo defect detection (deferred future feature)
- [ ] AI-generated audit summary (deferred future feature)
- [ ] AI recommendation for corrective action (deferred future feature)
- [x] Vendor assignment
- [x] SLA tracking
- [x] Cost tracking
- [x] Audit trend analysis

## 35. Final App Concept

- [x] Audit
- [x] Finding as first-class entity
- [x] Assignment
- [x] Corrective action via work orders
- [x] Verification
- [x] Score
- [x] Report
- [x] Management analysis

## Scope and Completion

All non-deferred core requirements are implemented for the web application and the offline
Android application. The Android app stores its own local SQLite data; synchronization with
the web application remains deferred. Email, WhatsApp, mobile push, CMMS, preventive
maintenance integrations, and AI features remain explicitly deferred in sections 28 and 34.

Verification evidence and environment limitations are recorded in `requirements-progress.md`.
