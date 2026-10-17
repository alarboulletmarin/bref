//! The views: each is a gpui entity (or the drawing of one) that talks to the shell through its
//! events. The editor, the side panel, the palette, the grid, the canvas, the graph, the viewer
//! and the kanban board.

pub mod canvas;
pub mod editor;
pub mod graph;
pub mod kanban;
pub mod nav;
pub mod palette;
pub mod sheet;
pub mod viewer;
