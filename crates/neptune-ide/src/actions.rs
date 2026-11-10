//! Every command the IDE can carry out. One table drives the menus, the
//! command palette and the keyboard.

use eframe::egui::{Key, KeyboardShortcut, Modifiers};

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Action {
    New,
    Open,
    OpenFolder,
    Save,
    SaveAs,
    SaveAll,
    Close,
    CloseAll,
    Quit,
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
    Find,
    FindNext,
    FindPrev,
    Replace,
    FindInFiles,
    ToggleComment,
    Indent,
    Outdent,
    DuplicateLine,
    DeleteLine,
    MoveLineUp,
    MoveLineDown,
    AddNextOccurrence,
    CaretAbove,
    CaretBelow,
    UpperCase,
    LowerCase,
    TrimTrailing,
    FormatFile,
    RenameSymbol,
    TriggerCompletion,
    Palette,
    ToggleSidebar,
    ToggleBottom,
    ToggleReference,
    ZoomIn,
    ZoomOut,
    ZoomReset,
    ToggleMinimap,
    ToggleWhitespace,
    ToggleFold,
    FoldAll,
    UnfoldAll,
    SplitEditor,
    Settings,
    NextTab,
    PrevTab,
    QuickOpen,
    GotoLine,
    GotoSymbol,
    GotoDefinition,
    Back,
    Forward,
    NextProblem,
    PrevProblem,
    ToggleBookmark,
    NextBookmark,
    PrevBookmark,
    Run,
    RunAgain,
    RunNoDebug,
    Stop,
    Check,
    Build,
    CommandLine,
    ToggleDebug,
    Continue,
    Pause,
    StepOver,
    StepIn,
    StepOut,
    ToggleBreakpoint,
    EditBreakpoint,
    ClearBreakpoints,
    QuickHelp,
    Reference,
    Manual,
    Welcome,
    Shortcuts,
    About,
}

const CTRL: Modifiers = Modifiers::COMMAND;
const NONE: Modifiers = Modifiers::NONE;
const SHIFT: Modifiers = Modifiers::SHIFT;
const ALT: Modifiers = Modifiers::ALT;
const CS: Modifiers = Modifiers { alt: false, ctrl: false, shift: true, mac_cmd: false, command: true };
const CA: Modifiers = Modifiers { alt: true, ctrl: false, shift: false, mac_cmd: false, command: true };
const AS: Modifiers = Modifiers { alt: true, ctrl: false, shift: true, mac_cmd: false, command: false };

pub struct Info {
    pub action: Action,
    pub title: &'static str,
    pub group: &'static str,
    pub keys: Option<(Modifiers, Key)>,
}

macro_rules! table {
    ($($a:ident, $group:literal, $title:literal $(, $m:ident + $k:ident)? ;)*) => {
        pub const ALL: &[Info] = &[$(Info { action: Action::$a, title: $title, group: $group, keys: table!(@k $($m $k)?) }),*];
    };
    (@k) => { None };
    (@k $m:ident $k:ident) => { Some(($m, Key::$k)) };
}

table! {
    New, "File", "New File", CTRL + N;
    Open, "File", "Open File...", CTRL + O;
    OpenFolder, "File", "Open Folder...", CS + O;
    Save, "File", "Save", CTRL + S;
    SaveAs, "File", "Save As...", CS + S;
    SaveAll, "File", "Save All", CA + S;
    Close, "File", "Close Tab", CTRL + W;
    CloseAll, "File", "Close All Tabs", CS + W;
    Quit, "File", "Exit";
    Undo, "Edit", "Undo", CTRL + Z;
    Redo, "Edit", "Redo", CTRL + Y;
    Cut, "Edit", "Cut";
    Copy, "Edit", "Copy";
    Paste, "Edit", "Paste";
    SelectAll, "Edit", "Select All", CTRL + A;
    Find, "Edit", "Find...", CTRL + F;
    FindNext, "Edit", "Find Next", NONE + F3;
    FindPrev, "Edit", "Find Previous", SHIFT + F3;
    Replace, "Edit", "Replace...", CTRL + H;
    FindInFiles, "Edit", "Find in Files...", CS + F;
    ToggleComment, "Edit", "Toggle Comment", CTRL + Slash;
    Indent, "Edit", "Indent Lines", CTRL + CloseBracket;
    Outdent, "Edit", "Outdent Lines", CTRL + OpenBracket;
    DuplicateLine, "Edit", "Duplicate Line", AS + ArrowDown;
    DeleteLine, "Edit", "Delete Line", CS + K;
    MoveLineUp, "Edit", "Move Line Up", ALT + ArrowUp;
    MoveLineDown, "Edit", "Move Line Down", ALT + ArrowDown;
    AddNextOccurrence, "Edit", "Add Next Occurrence", CTRL + D;
    CaretAbove, "Edit", "Add Caret Above", CA + ArrowUp;
    CaretBelow, "Edit", "Add Caret Below", CA + ArrowDown;
    UpperCase, "Edit", "Selection to Upper Case";
    LowerCase, "Edit", "Selection to Lower Case";
    TrimTrailing, "Edit", "Trim Trailing Spaces";
    FormatFile, "Edit", "Format File (Indent and Capitalise)", CS + I;
    RenameSymbol, "Edit", "Rename Symbol...", CTRL + R;
    TriggerCompletion, "Edit", "Show Completions", CTRL + Space;
    Palette, "View", "Command Palette...", CS + P;
    ToggleSidebar, "View", "Toggle Side Bar", CTRL + B;
    ToggleBottom, "View", "Toggle Bottom Panel", CTRL + J;
    ToggleReference, "View", "Toggle Command Reference", CS + R;
    ZoomIn, "View", "Zoom In", CTRL + Equals;
    ZoomOut, "View", "Zoom Out", CTRL + Minus;
    ZoomReset, "View", "Reset Zoom", CTRL + Num0;
    ToggleMinimap, "View", "Toggle Minimap", CS + M;
    ToggleWhitespace, "View", "Toggle Whitespace";
    ToggleFold, "View", "Fold or Unfold Block", CS + OpenBracket;
    FoldAll, "View", "Fold All";
    UnfoldAll, "View", "Unfold All", CS + CloseBracket;
    SplitEditor, "View", "Split Editor", CTRL + Backslash;
    Settings, "View", "Settings...", CTRL + Comma;
    NextTab, "View", "Next Tab", CTRL + Tab;
    PrevTab, "View", "Previous Tab", CS + Tab;
    QuickOpen, "Go", "Go to File...", CTRL + P;
    GotoLine, "Go", "Go to Line...", CTRL + G;
    GotoSymbol, "Go", "Go to Symbol...", CTRL + T;
    GotoDefinition, "Go", "Go to Definition", NONE + F12;
    Back, "Go", "Back", ALT + ArrowLeft;
    Forward, "Go", "Forward", ALT + ArrowRight;
    NextProblem, "Go", "Next Problem", NONE + F8;
    PrevProblem, "Go", "Previous Problem", SHIFT + F8;
    ToggleBookmark, "Go", "Toggle Bookmark", CTRL + F2;
    NextBookmark, "Go", "Next Bookmark", NONE + F2;
    PrevBookmark, "Go", "Previous Bookmark", SHIFT + F2;
    Run, "Program", "Run / Continue", NONE + F5;
    RunAgain, "Program", "Run Again", NONE + F6;
    RunNoDebug, "Program", "Run Without Debugger", CTRL + F5;
    Stop, "Program", "Stop", SHIFT + F5;
    Check, "Program", "Check for Errors", NONE + F7;
    Build, "Program", "Create Executable...", CTRL + F7;
    CommandLine, "Program", "Program Command Line...";
    ToggleDebug, "Program", "Debug Enabled";
    Continue, "Debug", "Continue";
    Pause, "Debug", "Pause", NONE + F4;
    StepOver, "Debug", "Step Over", NONE + F10;
    StepIn, "Debug", "Step Into", NONE + F11;
    StepOut, "Debug", "Step Out", SHIFT + F11;
    ToggleBreakpoint, "Debug", "Toggle Breakpoint", NONE + F9;
    EditBreakpoint, "Debug", "Breakpoint Condition...";
    ClearBreakpoints, "Debug", "Remove All Breakpoints";
    QuickHelp, "Help", "Quick Help for Word at Caret", NONE + F1;
    Reference, "Help", "Command Reference";
    Manual, "Help", "Manual";
    Welcome, "Help", "Welcome Page";
    Shortcuts, "Help", "Keyboard Shortcuts";
    About, "Help", "About Neptune IDE";
}

pub fn info(a: Action) -> &'static Info {
    ALL.iter().find(|i| i.action == a).expect("every action is in the table")
}

pub fn shortcut_of(a: Action) -> Option<KeyboardShortcut> {
    info(a).keys.map(|(m, k)| KeyboardShortcut::new(m, k))
}

/// `Ctrl+Shift+S`, for menus and the palette.
pub fn shortcut_text(a: Action) -> String {
    let Some((m, k)) = info(a).keys else { return String::new() };
    let mac = cfg!(target_os = "macos");
    let mut out = String::new();
    if m.command || m.ctrl {
        out.push_str(if mac { "Cmd+" } else { "Ctrl+" });
    }
    if m.alt {
        out.push_str(if mac { "Option+" } else { "Alt+" });
    }
    if m.shift {
        out.push_str("Shift+");
    }
    out.push_str(match k {
        Key::ArrowUp => "Up",
        Key::ArrowDown => "Down",
        Key::ArrowLeft => "Left",
        Key::ArrowRight => "Right",
        Key::Slash => "/",
        Key::Backslash => "\\",
        Key::OpenBracket => "[",
        Key::CloseBracket => "]",
        Key::Equals => "=",
        Key::Minus => "-",
        Key::Comma => ",",
        Key::Num0 => "0",
        other => other.name(),
    });
    out
}
