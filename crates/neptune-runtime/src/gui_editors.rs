//! Retained GUI (ext): numeric inputs, vector editors, colour pickers and fields, and curve editors. See `gui.rs` for the shared parts.
//!
//! Vector and colour widgets keep their components in `GuiItem::values`
//! (x,y,z or r,g,b,a with colours 0-255). A number box keeps its min and max
//! in `values[0..2]` so `SetGuiValue` can clamp like the widget does.
//! Changes the program makes itself are not counted by `GuiChanged`: the
//! stored values are updated first, so the widget's echo matches them.

use std::sync::Mutex;

use fyrox::{
    core::{
        algebra::Vector3,
        color::{Color, Hsv},
        math::curve::{Curve, CurveKey, CurveKeyKind},
        pool::Handle,
    },
    gui::{
        color::{
            AlphaBar, AlphaBarMessage, ColorField, ColorFieldBuilder, ColorFieldMessage, ColorPickerBuilder, ColorPickerMessage, HueBar, HueBarMessage,
            SaturationBrightnessField, SaturationBrightnessFieldMessage,
        },
        curve::{CurveEditorBuilder, CurveEditorMessage},
        message::UiMessage,
        numeric::{NumericUpDownBuilder, NumericUpDownMessage},
        vec::{Vec3EditorBuilder, Vec3EditorMessage},
        widget::WidgetBuilder,
        UiNode, UserInterface,
    },
};
use fyrox::graph::SceneGraph;
use neptune_vm::Value;

use crate::commands::Cmd;
use crate::gui::{rgba, GuiState};
use crate::world::Host3D;

fn int(v: i32) -> Result<Value, String> {
    Ok(Value::Int(v))
}
fn float(v: f32) -> Result<Value, String> {
    Ok(Value::Float(v))
}
const VOID: Result<Value, String> = Ok(Value::Null);

/// Copies of the curves in curve editors (the editor keeps its own private),
/// kept in sync from the editor's messages so `GuiCurveValue` works after edits.
static CURVES: Mutex<Vec<(Handle<UiNode>, Curve)>> = Mutex::new(Vec::new());

fn with_curve<R>(h: Handle<UiNode>, f: impl FnOnce(&mut Curve) -> R) -> Option<R> {
    let mut curves = CURVES.lock().unwrap_or_else(|e| e.into_inner());
    curves.iter_mut().find(|(c, _)| *c == h).map(|(_, c)| f(c))
}

fn store_curve(h: Handle<UiNode>, curve: Curve) {
    let mut curves = CURVES.lock().unwrap_or_else(|e| e.into_inner());
    match curves.iter_mut().find(|(c, _)| *c == h) {
        Some(slot) => slot.1 = curve,
        None => curves.push((h, curve)),
    }
}

fn color_values(c: Color) -> [f32; 4] {
    [c.r as f32, c.g as f32, c.b as f32, c.a as f32]
}

fn values_color(v: [f32; 4]) -> Color {
    rgba(v[0].round() as i32, v[1].round() as i32, v[2].round() as i32, v[3].round() as i32)
}

/// Stores new values and counts a change if they differ.
fn update(item: &mut crate::gui::GuiItem, v: [f32; 4]) {
    if item.values != v {
        item.values = v;
        item.changed += 1;
    }
}

/// Records what the user did with this module's widgets.
pub fn on_message(gui: &mut GuiState, id: i32, msg: &UiMessage) {
    let Ok(item) = gui.get_mut(id) else { return };
    match item.kind {
        "number" => {
            if let Some(&NumericUpDownMessage::Value(v)) = msg.data::<NumericUpDownMessage<f32>>() {
                if item.value != v {
                    item.value = v;
                    item.changed += 1;
                }
            }
        }
        "vector3" => {
            if let Some(&Vec3EditorMessage::<f32>::Value(v)) = msg.data::<Vec3EditorMessage<f32>>() {
                let w = item.values[3];
                update(item, [v.x, v.y, v.z, w]);
            }
        }
        "colorpicker" => match msg.data::<ColorPickerMessage>() {
            Some(&ColorPickerMessage::Color(c)) => update(item, color_values(c)),
            // the picker answers hue/saturation/brightness edits with HSV only; alpha is kept
            Some(&ColorPickerMessage::Hsv(hsv)) => {
                let c = Color::from(hsv);
                let a = item.values[3];
                update(item, [c.r as f32, c.g as f32, c.b as f32, a]);
            }
            None => {}
        },
        "colorfield" => {
            if let Some(&ColorFieldMessage::Color(c)) = msg.data::<ColorFieldMessage>() {
                update(item, color_values(c));
            }
        }
        "curve" => {
            if let Some(CurveEditorMessage::Sync(curves)) = msg.data::<CurveEditorMessage>() {
                store_curve(item.handle, curves.first().cloned().unwrap_or_default());
                item.changed += 1;
            }
        }
        _ => {}
    }
}

/// `SetGuiValue` for the widgets of this module.
pub fn set_value(h: &mut Host3D, id: i32, v: f32) -> Result<(), String> {
    let item = h.world.gui.get_mut(id)?;
    if item.kind != "number" {
        return Ok(());
    }
    // clamp here so the widget's echo matches and is not counted as a change
    let v = v.clamp(item.values[0], item.values[1]);
    item.value = v;
    let handle = item.handle;
    h.ui()?.send(handle, NumericUpDownMessage::Value(v));
    Ok(())
}

fn number(h: &mut Host3D, a: &[Value]) -> Result<i32, String> {
    let (min, max, step) = (a[1].float(), a[2].float(), a[3].float());
    let (min, max) = (min.min(max), max.max(min));
    let v = a[0].float().clamp(min, max);
    let id = h.gui_add("number", a[4].int(), |ctx| {
        let b = NumericUpDownBuilder::<f32>::new(WidgetBuilder::new().with_min_size(crate::gui::px2(80.0, 22.0)))
            .with_min_value(min)
            .with_max_value(max)
            .with_step(step)
            .with_value(v)
            // whole steps show whole numbers
            .with_precision(if step.fract() == 0.0 { 0 } else { 3 });
        (b.build(ctx).to_base(), Handle::NONE)
    })?;
    let item = h.world.gui.get_mut(id)?;
    item.value = v;
    item.values = [min, max, step, 0.0];
    Ok(id)
}

fn vector3(h: &mut Host3D, a: &[Value]) -> Result<i32, String> {
    let v = Vector3::new(a[0].float(), a[1].float(), a[2].float());
    let id = h.gui_add("vector3", a[3].int(), |ctx| {
        (Vec3EditorBuilder::<f32>::new(WidgetBuilder::new().with_min_size(crate::gui::px2(240.0, 22.0))).with_value(v).build(ctx).to_base(), Handle::NONE)
    })?;
    h.world.gui.get_mut(id)?.values = [v.x, v.y, v.z, 0.0];
    Ok(id)
}

/// A colour that differs from `c`, to build a picker with: Fyrox's pickers
/// only show a colour (its swatch, hue bar and square) when it is changed.
fn other_color(c: Color) -> Color {
    Color::from_rgba(c.r ^ 1, c.g, c.b, c.a)
}

/// Shows `c` in a colour picker: sets the colour, then the hue bar, the
/// saturation/brightness square and the alpha bar, which Fyrox leaves alone.
fn sync_picker(ui: &mut UserInterface, picker: Handle<UiNode>, c: Color) {
    ui.send(picker, ColorPickerMessage::Color(c));
    let hsv = Hsv::from(c);
    let parts: Vec<Handle<UiNode>> = ui.traverse_handle_iter(picker).collect();
    for n in parts {
        if ui.try_get_of_type::<HueBar>(n).is_ok() {
            ui.send(n, HueBarMessage::Hue(hsv.hue()));
        } else if ui.try_get_of_type::<SaturationBrightnessField>(n).is_ok() {
            ui.send(n, SaturationBrightnessFieldMessage::Hue(hsv.hue()));
            ui.send(n, SaturationBrightnessFieldMessage::Saturation(hsv.saturation()));
            ui.send(n, SaturationBrightnessFieldMessage::Brightness(hsv.brightness()));
        } else if ui.try_get_of_type::<AlphaBar>(n).is_ok() {
            ui.send(n, AlphaBarMessage::Alpha(c.a as f32));
        }
    }
}

/// The picker inside a colour field's popup.
fn field_picker(ui: &UserInterface, field: Handle<UiNode>) -> Option<Handle<UiNode>> {
    ui.try_get_of_type::<ColorField>(field).ok().map(|f| f.picker.to_base())
}

fn color_widget(h: &mut Host3D, a: &[Value], field: bool) -> Result<i32, String> {
    let c = rgba(a[0].int(), a[1].int(), a[2].int(), 255);
    let id = if field {
        h.gui_add("colorfield", a[3].int(), |ctx| {
            (ColorFieldBuilder::new(WidgetBuilder::new().with_min_size(crate::gui::px2(60.0, 22.0))).with_color(c).build(ctx).to_base(), Handle::NONE)
        })?
    } else {
        // the saturation/brightness square stretches, so the picker needs a size
        let wb = WidgetBuilder::new().with_width(crate::gui::px(320.0)).with_height(crate::gui::px(200.0));
        h.gui_add("colorpicker", a[3].int(), |ctx| (ColorPickerBuilder::new(wb).with_color(other_color(c)).build(ctx).to_base(), Handle::NONE))?
    };
    let item = h.world.gui.get_mut(id)?;
    item.values = color_values(c);
    let handle = item.handle;
    let ui = h.ui()?;
    let picker = if field { field_picker(ui, handle) } else { Some(handle) };
    if let Some(p) = picker {
        if field {
            ui.send(p, ColorPickerMessage::Color(other_color(c)));
        }
        sync_picker(ui, p, c);
    }
    Ok(id)
}

/// `SetGuiVector`: x,y,z of a vector editor or r,g,b,a of a colour widget.
fn set_vector(h: &mut Host3D, id: i32, v: [f32; 4]) -> Result<(), String> {
    let item = h.world.gui.get_mut(id)?;
    let (kind, handle) = (item.kind, item.handle);
    match kind {
        "vector3" => {
            item.values = [v[0], v[1], v[2], 0.0];
            h.ui()?.send(handle, Vec3EditorMessage::<f32>::Value(Vector3::new(v[0], v[1], v[2])));
        }
        "colorpicker" | "colorfield" => {
            let c = values_color(v);
            item.values = color_values(c);
            let ui = h.ui()?;
            if kind == "colorfield" {
                ui.send(handle, ColorFieldMessage::Color(c));
                if let Some(p) = field_picker(ui, handle) {
                    sync_picker(ui, p, c);
                }
            } else {
                sync_picker(ui, handle, c);
            }
        }
        _ => return Err("Not a vector or colour widget".into()),
    }
    Ok(())
}

fn vector_value(h: &mut Host3D, id: i32, i: i32) -> Result<f32, String> {
    let item = h.world.gui.get(id)?;
    if !matches!(item.kind, "vector3" | "colorpicker" | "colorfield") {
        return Err("Not a vector or colour widget".into());
    }
    item.values.get(i as usize).copied().ok_or_else(|| "Component must be 0 to 3".to_string())
}

fn curve_handle(h: &mut Host3D, id: i32) -> Result<Handle<UiNode>, String> {
    let item = h.world.gui.get(id)?;
    if item.kind != "curve" {
        return Err("Not a curve widget".into());
    }
    Ok(item.handle)
}

fn curve(h: &mut Host3D, parent: i32) -> Result<i32, String> {
    let c = Curve::from(vec![CurveKey::new(0.0, 0.0, CurveKeyKind::Linear), CurveKey::new(1.0, 1.0, CurveKeyKind::Linear)]);
    let start = c.clone();
    let id = h.gui_add("curve", parent, |ctx| {
        let e = CurveEditorBuilder::new(WidgetBuilder::new().with_min_size(crate::gui::px2(300.0, 160.0))).with_curves(vec![start]).build(ctx).to_base();
        (e, Handle::NONE)
    })?;
    let handle = h.world.gui.get(id)?.handle;
    store_curve(handle, c);
    let ui = h.ui()?;
    // the editor would otherwise draw its first curve faintly behind later ones
    ui.send(handle, CurveEditorMessage::ShowBackgroundCurves(false));
    ui.send(handle, CurveEditorMessage::ZoomToFit { after_layout: true });
    Ok(id)
}

/// Changes the stored copy of a curve and shows it in the editor.
fn edit_curve(h: &mut Host3D, id: i32, f: impl FnOnce(&mut Curve)) -> Result<(), String> {
    let handle = curve_handle(h, id)?;
    let c = with_curve(handle, |c| {
        f(c);
        c.clone()
    })
    .ok_or("Curve does not exist")?;
    let ui = h.ui()?;
    ui.send(handle, CurveEditorMessage::Sync(vec![c]));
    ui.send(handle, CurveEditorMessage::ZoomToFit { after_layout: true });
    Ok(())
}

fn curve_value(h: &mut Host3D, id: i32, x: f32) -> Result<f32, String> {
    let handle = curve_handle(h, id)?;
    with_curve(handle, |c| c.value_at(x)).ok_or_else(|| "Curve does not exist".to_string())
}

pub static COMMANDS: &[(&str, Cmd)] = &[
    // ---- GUI (ext): editors
    ("%GuiNumber#value#min=-1000000#max=1000000#step=1%parent=0", |h, a, _| int(number(h, a)?)),
    ("%GuiVector3#x#y#z%parent=0", |h, a, _| int(vector3(h, a)?)),
    ("%GuiColorPicker%red=255%green=255%blue=255%parent=0", |h, a, _| int(color_widget(h, a, false)?)),
    ("%GuiColorField%red%green%blue%parent=0", |h, a, _| int(color_widget(h, a, true)?)),
    ("#GuiVectorValue%widget%component", |h, a, _| float(vector_value(h, a[0].int(), a[1].int())?)),
    ("SetGuiVector%widget#x#y#z#w=255", |h, a, _| {
        set_vector(h, a[0].int(), [a[1].float(), a[2].float(), a[3].float(), a[4].float()])?;
        VOID
    }),
    ("%GuiCurve%parent=0", |h, a, _| int(curve(h, a[0].int())?)),
    ("GuiCurveKey%curve#x#y", |h, a, _| {
        let (x, y) = (a[1].float(), a[2].float());
        edit_curve(h, a[0].int(), |c| c.add_key(CurveKey::new(x, y, CurveKeyKind::Linear)))?;
        VOID
    }),
    ("GuiClearCurve%curve", |h, a, _| {
        edit_curve(h, a[0].int(), Curve::clear)?;
        VOID
    }),
    ("#GuiCurveValue%curve#x", |h, a, _| float(curve_value(h, a[0].int(), a[1].float())?)),
];
