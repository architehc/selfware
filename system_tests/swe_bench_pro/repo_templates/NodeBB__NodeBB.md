# NodeBB/NodeBB-specific instructions

- Do NOT create new source files under `src/database/`, `src/controllers/`, `src/topics/`, `src/posts/`, `src/user/`, or any other existing source directory. Modify the existing files named in the issue/requirements.
- When the issue mentions language files (e.g. `public/language/en-GB/...`), edit those JSON files in place; do not create duplicate language files elsewhere.
- Prefer small, localized edits. Keep existing function signatures, exports, and file structure intact.
