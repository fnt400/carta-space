# OpenCode verification report

- Tested HEAD: `1fef1f0a04f3da08f2d39cbce85e0822c6a28437`
- Branch: `opencode/v0.2`
- Result: **FAIL — verification not run**

## Checks

The initial required precondition, a clean working tree, failed:

```text
git status --short --branch
## opencode/v0.2...origin/opencode/v0.2
?? crates/carta-gui/Cargo.lock
?? crates/carta-gui/target/
```

Because the tree was not clean, the required `git pull --ff-only origin opencode/v0.2` and all workspace, GUI, and formatting checks were not run. No code or test failure was observed; the verification is blocked by the untracked paths above.
