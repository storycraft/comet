# comet-tree
An archetypal UI tree backed by `hecs`. Each element has a `Node` kind (`Box`
or `Text`) and typed `StyleProp` components. The `Text` property stores text
content for text elements.

## Examples
See [tree.rs](examples/tree.rs) for a UI tree printed with `<box>` and `<text>` tags.

Run it with `cargo run -p comet-tree --example tree`.

## License
comet-tree is licensed under [Apache-2.0](../../LICENSE).
