//! The app's actions: what the shortcuts, the menus and the palette dispatch. The views
//! dispatch them, the shell handles them.

use gpui::actions;

actions!(app, [ToggleCommands, SyncVault, SplitPane, SplitDown, FocusLeft, FocusRight, PaneLeft, PaneRight, PaneUp, PaneDown, TabToLeft, TabToRight, TabToUp, TabToDown, EvenPanes, MaximizePane, ClosePane, ReopenTab, NextTab, PrevTab, MoveTabLeft, MoveTabRight, GoBack, GoForward, SearchVault, Today, Outline, OpenPalette, NewNote, NewFromTemplate, NewDiagram, OpenVault, CopyAll, ToggleHelp, CloseHelp, ChooseTheme, ContextMenu, FocusNext, FocusPrev, ZoomIn, ZoomOut, ZoomReset, Quit]);
