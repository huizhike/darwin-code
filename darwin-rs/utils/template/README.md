# darwin-code-utils-template

Small, strict string templating for prompt and text assets.

Supported syntax:

- `{{ name }}` placeholder interpolation
- `{{{{` for a literal `{{`
- `}}}}` for a literal `}}`

The library is intentionally strict:

- parsing fails on malformed placeholders
- rendering fails on missing values
- rendering fails on duplicate values
- rendering fails on extra values not used by the template

## Example

```rust
use darwin_code_utils_template::Template;
use darwin_code_utils_template::render;

let template = Template::parse(
    "Hello, {{ name }}.\nLiteral braces: {{{{ and }}}}.\nMode: {{ mode }}",
)?;

let rendered = template.render([
    ("name", "Darwin Code"),
    ("mode", "strict"),
])?;

assert_eq!(
    rendered,
    "Hello, Darwin Code.\nLiteral braces: {{ and }}.\nMode: strict"
);

let one_shot = render("Hi {{ who }}!", [("who", "there")])?;
assert_eq!(one_shot, "Hi there!");
# Ok::<(), Box<dyn std::error::Error>>(())
```
