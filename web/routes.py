"""HTTP mutation dispatch, grouped by domain in routes_*.py."""
from routes_accounts import delete_roles, delete_users, patch_account, patch_roles, patch_users, post_auth_change_password, post_auth_forgot_password, post_auth_login, post_auth_logout, post_auth_register, post_roles, post_users
from routes_assets import delete_equipment, patch_equipment, post_equipment
from routes_inspections import delete_inspection_sessions, delete_schedules, patch_inspection_sessions, patch_schedules, post_audits, post_captain_logins, post_inspection_sessions, post_inspections, post_schedules
from routes_inspections import start_schedule
from routes_locations import delete_locations, delete_setup_outlets, delete_zones, patch_locations, patch_setup_outlets, patch_zones, post_locations, post_setup_outlets, post_zones
from routes_setup import delete_setup_audit_types, delete_setup_categories, delete_setup_departments, delete_setup_priorities, patch_setup_audit_types, patch_setup_categories, patch_setup_departments, patch_setup_priorities, post_settings, post_setup_audit_types, post_setup_categories, post_setup_departments, post_setup_priorities
from routes_work_orders import delete_comments, delete_notifications, delete_work_orders, patch_notifications, patch_work_orders, post_comments, post_notifications, post_work_orders

ROUTES = {
    'POST': {
        '/api/auth/login': post_auth_login,
        '/api/auth/logout': post_auth_logout,
        '/api/auth/forgot-password': post_auth_forgot_password,
        '/api/auth/register': post_auth_register,
        '/api/auth/change-password': post_auth_change_password,
        '/api/audits': post_audits,
        '/api/inspections': post_inspections,
        '/api/inspection-sessions': post_inspection_sessions,
        '/api/schedules': post_schedules,
        '/api/schedules/start': start_schedule,
        '/api/captain-logins': post_captain_logins,
        '/api/work-orders': post_work_orders,
        '/api/equipment': post_equipment,
        '/api/locations': post_locations,
        '/api/zones': post_zones,
        '/api/setup/departments': post_setup_departments,
        '/api/setup/categories': post_setup_categories,
        '/api/setup/outlets': post_setup_outlets,
        '/api/users': post_users,
        '/api/roles': post_roles,
        '/api/setup/priorities': post_setup_priorities,
        '/api/setup/audit-types': post_setup_audit_types,
        '/api/settings': post_settings,
        '/api/comments': post_comments,
        '/api/notifications': post_notifications,
    },
    'PATCH': {
        '/api/account': patch_account,
        '/api/inspection-sessions': patch_inspection_sessions,
        '/api/users': patch_users,
        '/api/roles': patch_roles,
        '/api/setup/priorities': patch_setup_priorities,
        '/api/setup/audit-types': patch_setup_audit_types,
        '/api/notifications': patch_notifications,
        '/api/setup/departments': patch_setup_departments,
        '/api/setup/categories': patch_setup_categories,
        '/api/setup/outlets': patch_setup_outlets,
        '/api/equipment': patch_equipment,
        '/api/work-orders': patch_work_orders,
        '/api/locations': patch_locations,
        '/api/zones': patch_zones,
        '/api/schedules': patch_schedules,
    },
    'DELETE': {
        '/api/inspection-sessions': delete_inspection_sessions,
        '/api/setup/departments': delete_setup_departments,
        '/api/setup/categories': delete_setup_categories,
        '/api/setup/priorities': delete_setup_priorities,
        '/api/setup/audit-types': delete_setup_audit_types,
        '/api/notifications': delete_notifications,
        '/api/comments': delete_comments,
        '/api/setup/outlets': delete_setup_outlets,
        '/api/users': delete_users,
        '/api/roles': delete_roles,
        '/api/locations': delete_locations,
        '/api/zones': delete_zones,
        '/api/equipment': delete_equipment,
        '/api/work-orders': delete_work_orders,
        '/api/schedules': delete_schedules,
    },
}

def dispatch(method, handler, parsed, payload=None):
    path = parsed.path if method == "POST" else parsed.path.rsplit("/", 1)[0]
    route = ROUTES[method].get(parsed.path) or ROUTES[method].get(path)
    if route is None:
        return False
    route(handler, parsed, payload)
    return True
