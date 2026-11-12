---
name: quickstart-from-release-download
description: Every product ships ONE self-contained compose.yaml as a release asset; the quick start curls it, never clones and never unpacks a bundle
metadata:
  type: feedback
---

Every product has one self-contained `compose.yaml`. Its configs and init scripts are inline under Compose `configs:` with `content:`, and its images are pinned by digest. The release attaches that one file as an asset. The quick start (README and book) is `curl -fsSLO https://github.com/FerroHEALTH/<Repo>/releases/latest/download/compose.yaml`, then a few openssl lines for the secret files, then `docker compose up --wait`. No `git clone`, no tarball bundle, no `{{…}}` marker in a README.

**Why:** the owner, 2026-11-10: cloning the repo to run a server is wrong ("do curl and get the last release compose.yaml"), and a quickstart bundle per product adds needless complexity; one compose.yaml is "the most simple and best direction and fastest for everybody". Cloning main also ran main's config against an older image.

**How to apply:** the CI tour runs against the repo's compose.yaml exactly as shipped. Licensed data (terminology) stays an optional user-supplied mount. The README switch lands in the same milestone as the release-asset step. Related: [[websites-consistency-pass]].
