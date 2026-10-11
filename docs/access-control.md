# Portal access control

Open **Access control** to manage roles, user groups and signed-in users.
The **Manage users, groups and all role grants** action is required. Anyone
with this action can grant any role, including Owner; assign it only to access
administrators. The local recovery administrator always retains access.

## Default roles

| Role | Initial permissions |
| --- | --- |
| Owner | Every portal action; reserved and cannot be edited or deleted |
| Admin | Every portal action; editable |
| Policy Editor | View dashboard, catalogue and policies; edit policies and DLP; submit approvals; export policy documents |
| Approver | View dashboard, catalogue and policies; approve another person's changes |
| Auditor | View dashboard, catalogue, policies, settings and audit; export audit |
| Viewer | View dashboard and catalogue |

Each action is a separate checkbox on a role. Create a custom role to grant,
for example, View settings and Change enforcement mode without granting Change
settings and kill switches. Include the relevant View action so the user can
open the page containing an action. Roles combine across all matching groups.
No group match means no portal access; there is no implicit Viewer fallback.

## Assigning people

1. Have the person sign in once. Their name, email and stable identity-provider
   user ID appear in Users, even if they do not yet have access.
2. Create or edit a user group. Select its roles and people, then save it.
3. Alternatively, enter exact identity-provider group IDs or app-role values,
   one per line. People presenting those claims at sign-in join that group.

Membership uses the stable user ID rather than an editable email address.
Creating a role or group does not create credentials. Authentication remains
with the configured identity provider; the existing local administrator is
reserved for recovery. The SCIM example directory is separate from portal
access groups; use the verified sign-in group claims for IdP assignments.

Role grants and local group membership are evaluated on every action, so
changes apply to active sessions. Changed IdP claims require a new sign-in.
Disable a user to deny access immediately and revoke their sessions. Re-enabling
requires a fresh sign-in. You cannot disable yourself or the recovery account.
The UI prevents an access administrator from removing their own access-management
permission; another administrator can perform that removal.

Delete a group's role assignment before deleting that role. The Owner role
always keeps all actions. Built-in roles other than Owner and their groups can
be edited. Existing `MAGPIE_ROLE_*` mappings seed the initial groups; after that,
the UI owns them. Changes survive restarts and redeployments.

## Enforcement and audit

Use **Settings → Guardrail mode → Apply mode** to switch Log-only or Enforcement
without a deployment. This requires Change enforcement mode and is independent
of permission to edit policy settings. It affects new requests immediately;
in-flight requests retain their original decision. Mode changes and all access
changes are committed atomically with their audit events.

Runtime mode, roles, groups and portal users are stored separately from imported
policy documents. Policy imports and rollbacks cannot overwrite them. Imports
are restricted to policy, DLP, approval, provider, agent and argument-rule
documents. The import action can activate those documents directly and is
therefore initially granted only to Owner and Admin.

Back up the SQLite database to preserve access configuration and users; the
policy JSON export does not include these runtime controls. The new tables are
created automatically during upgrade. Existing deployments start with their
current deployment mode and role mappings. Existing SSO sessions may need to
sign in again so their user records and verified group claims can be registered.
