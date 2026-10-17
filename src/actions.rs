//! The app's actions: what the shortcuts, the menus and the palette dispatch. The views
//! dispatch them, the shell handles them.

use gpui::actions;

actions!(app, [ToggleCommands, SyncVault, SplitPane, FocusLeft, FocusRight, ClosePane, GoBack, GoForward, SearchVault, Today, Outline, OpenPalette, NewNote, NewFromTemplate, NewDiagram, OpenVault, CopyAll, ToggleHelp, CloseHelp, ChooseTheme, ZoomIn, ZoomOut, ZoomReset, Quit]);
