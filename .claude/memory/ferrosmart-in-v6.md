---
name: ferrosmart-in-v6
description: Owner decisions 2026-11-16 on FerroSMART, the family's own SMART App Launch 2.x authorization server
metadata:
  type: project
---

FerroSMART is the family's own SMART App Launch 2.x authorization server (owner, 2026-11-16).

- **Location:** permanently in the FerroAUTH repository, shipped as its own OCI image. No new repo.
- **Licence:** BUSL-1.1.
- **Upstream IdPs:** Keycloak, ZITADEL, Entra ID and Okta are used only for login behind it.
- **Release:** required for FerroEHR v6.0.0. v6.0.0-rc1 waits for it.

**Why:** no IdP issues real SMART tokens. ZITADEL drops SMART scopes and `launch` (FerroEHR #5827), and Keycloak as configured has fixed scopes and no launch context. The owner wants SMART to work properly for any IdP, and fewer scattered products.

**How to apply:**
- Lifts FerroAUTH's former "never an authorization server" rule.
- Standards-only and vendor neutral on every interface.
- Policy (role to scope, clients, branding) is config, never code.
- Related: [[ferroehr-v6-major]], [[cdr-mechanism-app-policy]].
