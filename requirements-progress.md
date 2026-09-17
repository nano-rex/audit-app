# Requirements completion work

Baseline: `req_spec_checklist.md`. A checkbox will be promoted only with implementation and verification evidence.

Work in progress:

1. Preserve finding classification, department, PIC, comments, and its work-order relationship through draft/save/submit.
2. Store original/marked/completion photos and signatures as private image files; render actual images in multi-page PDF reports.
3. Enforce corrective-action transitions, verification, and assigned-user access; make scoring configuration effective.
4. Complete account/session/profile flows, audit metadata, hierarchy, reporting filters and exports, notifications, and mobile access.
5. Verify each core checklist section and record external-service dependencies separately.

Confirmed scope: complete core requirements; section 34 external/future integrations remain separate. Android keeps working offline; synchronization is deferred.

## Implemented and checked in this increment

- Extracted the Python backend into focused modules with an explicit mutation route registry.
- Failed audit criteria produce one linked finding and work order on submission; draft finding
  details retain priority, department, PIC, cause, recommendation, and required action.
- Original and marked photos and signatures can be stored as validated private image files.
  Work-order edits retain evidence and diagnostic fields.
- PDF reports paginate the full checklist and embed actual evidence/signature images.
- Audit submission records a scoring snapshot, excludes N/A criteria, and supports configured
  category weights in the calculation. The category-weight editor remains to be completed.
- Session API saves time, type, and remarks. Completing the corresponding UI remains part of core work.
- Dialog close buttons no longer submit forms.

Evidence: 16 Python tests, 4 frontend tests, and a 100-user/400-request synthetic load run with
zero errors and 100 drafts saved. See `backend-architecture.md` for commands and limitations.

This increment does not certify the full checklist complete. Workflow authorization/transitions,
account flows, hierarchy consistency, remaining report controls/exports, notifications, and
offline Android requirements still need the implementation and verification listed above.
