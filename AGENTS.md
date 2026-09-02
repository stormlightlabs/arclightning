- Include rustdoc/tsdoc comments for exported & contextually important symbols
- In Rust code, never separate impls from their types and avoid `unsafe`
  - Order: constants, enums, structs, then free functions. pub before private/local
  - Use only 1 level of super (i.e. no `super::super::`)
- Use the writing skill and keep communication direct & succinct while retaining
  the user's voice. Avoid legacy/remains type language.
- Linting must clear even if warnings are unrelated.
