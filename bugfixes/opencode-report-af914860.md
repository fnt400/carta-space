# Verification Failure

Tested HEAD: `af91486055b4a105394e0ab3294fddc8c59d4a0d`

`cargo fmt --check` failed in `crates/carta-tui/src/input.rs:220`. Rustfmt requires the Home/End match alternatives on one line:

```diff
-            (LeapDirection::Backward, KeyCode::Home)
-            | (LeapDirection::Forward, KeyCode::End) => {
+            (LeapDirection::Backward, KeyCode::Home) | (LeapDirection::Forward, KeyCode::End) => {
```
