# Comet
Comet is a Rust GUI library.

## Core concepts
1. A `Node` is one of text or div and is a single available building gui block in Comet. Without introducing unnecessary `View` or `Widget` like traits.
2. A `Node` is composed with multiple props. By using archetypal data structure from borrowed from ECS, 
   1. It's horizontally scalable.
   2. It can be grouped in memory.
   3. Props can be queried efficiently.
3. Rich text is included and subset of css `block`/`inline` layout can be used.
4. Any stateless container layout algorithm can be applied to a `Node` via `ContainerLayoutFn`.

## License
Comet is licensed under Apache-2.0