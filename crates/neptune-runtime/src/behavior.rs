//! Behaviour trees (ext): decision making for characters.
//!
//! A tree is built from four kinds of node:
//!
//! * **selector** – tries its children in order until one succeeds or runs
//!   (the first that applies wins: "chase if close, else patrol");
//! * **sequence** – runs its children in order while they succeed ("is the
//!   player close?" then "chase");
//! * **invert** – turns its child's success into failure and back;
//! * **condition** – compares parameters the program sets, such as
//!   `dist < 10` or `health >= max_health`;
//! * **action** – something the program does. The program asks which
//!   action is current (`BehaviorAction`), does it every frame, and reports
//!   when it has finished (`BehaviorResult`).
//!
//! Trees are evaluated again every time the program asks, so a branch with
//! a higher priority takes over as soon as its conditions hold. Trees are
//! built with commands or written as indented text (`LoadBehavior`):
//!
//! ```text
//! selector
//!   sequence
//!     if dist < 10
//!     do chase
//!   do patrol
//! ```

use std::collections::HashMap;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Kind {
    Selector,
    Sequence,
    Invert,
    Condition,
    Action,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub kind: Kind,
    /// The condition or action text.
    pub text: String,
    pub children: Vec<usize>,
}

#[derive(Debug, Clone, Default)]
pub struct Tree {
    pub name: String,
    pub nodes: Vec<Node>,
    /// Nodes without a parent; the first is the root.
    pub roots: Vec<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Status {
    Success,
    Failure,
    Running,
}

/// One character running a tree.
#[derive(Debug, Clone, Default)]
pub struct Instance {
    pub tree: i32,
    pub params: HashMap<String, f32>,
    /// The action node now running.
    pub running: Option<usize>,
    /// A result the program reported for an action node.
    pub result: Option<(usize, bool)>,
}

impl Tree {
    /// Adds a node under `parent` (1-based node id, 0 = top level); returns its id.
    pub fn add(&mut self, kind: Kind, text: &str, parent: i32) -> Result<i32, String> {
        let id = self.nodes.len();
        if parent > 0 {
            let p = self.nodes.get(parent as usize - 1).ok_or_else(|| format!("Behavior node {parent} does not exist"))?;
            if matches!(p.kind, Kind::Condition | Kind::Action) {
                return Err("conditions and actions cannot have children".into());
            }
            if p.kind == Kind::Invert && !p.children.is_empty() {
                return Err("an invert node has one child".into());
            }
        }
        if kind == Kind::Condition {
            parse_condition(text)?;
        }
        self.nodes.push(Node { kind, text: text.trim().to_string(), children: Vec::new() });
        if parent > 0 {
            self.nodes[parent as usize - 1].children.push(id);
        } else {
            self.roots.push(id);
        }
        Ok(id as i32 + 1)
    }

    /// Reads a tree written as indented text (`selector`, `sequence`,
    /// `invert`, `if <condition>`, `do <action>`; `;` starts a comment).
    pub fn parse(name: &str, text: &str) -> Result<Tree, String> {
        let mut tree = Tree { name: name.to_string(), ..Default::default() };
        // (indent, node id) of the open parents
        let mut stack: Vec<(usize, i32)> = Vec::new();
        for (n, raw) in text.lines().enumerate() {
            let line = raw.split(';').next().unwrap_or("");
            if line.trim().is_empty() {
                continue;
            }
            let indent = line.len() - line.trim_start().len();
            let body = line.trim();
            let (word, rest) = body.split_once(char::is_whitespace).map_or((body, ""), |(w, r)| (w, r.trim()));
            let kind = match word.to_ascii_lowercase().as_str() {
                "selector" | "select" | "fallback" => Kind::Selector,
                "sequence" => Kind::Sequence,
                "invert" | "not" => Kind::Invert,
                "if" | "condition" => Kind::Condition,
                "do" | "action" => Kind::Action,
                _ => return Err(format!("behavior line {}: unknown node '{word}'", n + 1)),
            };
            if matches!(kind, Kind::Condition | Kind::Action) && rest.is_empty() {
                return Err(format!("behavior line {}: '{word}' needs text after it", n + 1));
            }
            while stack.last().is_some_and(|&(i, _)| i >= indent) {
                stack.pop();
            }
            let parent = stack.last().map_or(0, |&(_, id)| id);
            let id = tree.add(kind, rest, parent).map_err(|e| format!("behavior line {}: {e}", n + 1))?;
            stack.push((indent, id));
        }
        if tree.roots.is_empty() {
            return Err("the behavior is empty".into());
        }
        Ok(tree)
    }

    fn tick(&self, node: usize, inst: &mut Instance) -> Status {
        let n = &self.nodes[node];
        match n.kind {
            Kind::Selector => {
                for &c in &n.children {
                    match self.tick(c, inst) {
                        Status::Failure => {}
                        s => return s,
                    }
                }
                Status::Failure
            }
            Kind::Sequence => {
                for &c in &n.children {
                    match self.tick(c, inst) {
                        Status::Success => {}
                        s => return s,
                    }
                }
                Status::Success
            }
            Kind::Invert => match n.children.first().map(|&c| self.tick(c, inst)) {
                Some(Status::Success) => Status::Failure,
                Some(Status::Failure) | None => Status::Success,
                Some(Status::Running) => Status::Running,
            },
            Kind::Condition => {
                if parse_condition(&n.text).is_ok_and(|c| c.holds(&inst.params)) {
                    Status::Success
                } else {
                    Status::Failure
                }
            }
            Kind::Action => match inst.result {
                Some((id, ok)) if id == node => {
                    inst.result = None;
                    if inst.running == Some(node) {
                        inst.running = None;
                    }
                    if ok {
                        Status::Success
                    } else {
                        Status::Failure
                    }
                }
                _ => {
                    inst.running = Some(node);
                    Status::Running
                }
            },
        }
    }

    /// Evaluates the tree; returns the action now running, if any.
    pub fn update(&self, inst: &mut Instance) -> Option<usize> {
        let root = *self.roots.first()?;
        // a finished tree starts again at once, so an action is always ready
        for _ in 0..2 {
            inst.running = None;
            let status = self.tick(root, inst);
            // a result for an action that was not reached is dropped
            inst.result = None;
            if status == Status::Running {
                return inst.running;
            }
        }
        None
    }
}

#[derive(Debug, Clone, PartialEq)]
enum Operand {
    Num(f32),
    Param(String),
}

impl Operand {
    fn value(&self, params: &HashMap<String, f32>) -> f32 {
        match self {
            Operand::Num(v) => *v,
            Operand::Param(p) => params.get(p).copied().unwrap_or(0.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Condition {
    left: Operand,
    op: &'static str,
    right: Operand,
}

impl Condition {
    fn holds(&self, params: &HashMap<String, f32>) -> bool {
        let (a, b) = (self.left.value(params), self.right.value(params));
        match self.op {
            "<" => a < b,
            "<=" => a <= b,
            ">" => a > b,
            ">=" => a >= b,
            "=" => a == b,
            "<>" => a != b,
            _ => a != 0.0,
        }
    }
}

fn operand(s: &str) -> Result<Operand, String> {
    let s = s.trim();
    if s.is_empty() {
        return Err("a condition is missing a value".into());
    }
    if let Ok(v) = s.parse::<f32>() {
        return Ok(Operand::Num(v));
    }
    if s.chars().all(|c| c.is_alphanumeric() || c == '_') {
        return Ok(Operand::Param(s.to_ascii_lowercase()));
    }
    Err(format!("'{s}' is not a number or parameter name"))
}

/// `name`, or `a op b` with op one of `< <= > >= = <>` (also `==`, `!=`).
fn parse_condition(text: &str) -> Result<Condition, String> {
    for (pat, op) in [("<=", "<="), (">=", ">="), ("<>", "<>"), ("!=", "<>"), ("==", "="), ("<", "<"), (">", ">"), ("=", "=")] {
        if let Some((l, r)) = text.split_once(pat) {
            return Ok(Condition { left: operand(l)?, op, right: operand(r)? });
        }
    }
    Ok(Condition { left: operand(text)?, op: "", right: Operand::Num(0.0) })
}

/// Parameter names are not case-sensitive, like BASIC names.
pub fn param_name(s: &str) -> String {
    s.trim().to_ascii_lowercase()
}

impl crate::world::Host3D<'_> {
    pub fn behavior(&self, t: i32) -> Result<&Tree, String> {
        usize::try_from(t - 1).ok().and_then(|i| self.world.behaviors.get(i)).and_then(Option::as_ref).ok_or_else(|| format!("Behavior {t} does not exist"))
    }

    pub fn behavior_mut(&mut self, t: i32) -> Result<&mut Tree, String> {
        usize::try_from(t - 1).ok().and_then(|i| self.world.behaviors.get_mut(i)).and_then(Option::as_mut).ok_or_else(|| format!("Behavior {t} does not exist"))
    }

    pub fn add_behavior(&mut self, tree: Tree) -> i32 {
        self.world.behaviors.push(Some(tree));
        self.world.behaviors.len() as i32
    }

    /// `SetBehavior`: gives a character (any number, usually an entity) a tree; 0 removes it.
    pub fn set_behavior(&mut self, who: i32, tree: i32) -> Result<(), String> {
        if tree == 0 {
            self.world.behavior_of.remove(&who);
            return Ok(());
        }
        self.behavior(tree)?;
        let inst = self.world.behavior_of.entry(who).or_default();
        inst.tree = tree;
        inst.running = None;
        inst.result = None;
        Ok(())
    }

    fn instance(&mut self, who: i32) -> Result<&mut Instance, String> {
        self.world.behavior_of.get_mut(&who).ok_or_else(|| format!("{who} has no behavior (use SetBehavior)"))
    }

    pub fn set_behavior_param(&mut self, who: i32, name: &str, v: f32) -> Result<(), String> {
        self.instance(who)?.params.insert(param_name(name), v);
        Ok(())
    }

    pub fn behavior_param(&mut self, who: i32, name: &str) -> Result<f32, String> {
        Ok(self.instance(who)?.params.get(&param_name(name)).copied().unwrap_or(0.0))
    }

    /// `BehaviorAction`: evaluates the character's tree; the action to do now, or "".
    pub fn behavior_action(&mut self, who: i32) -> Result<String, String> {
        let t = self.instance(who)?.tree;
        let tree = self.behavior(t)?.clone();
        let inst = self.instance(who)?;
        Ok(tree.update(inst).map(|n| tree.nodes[n].text.clone()).unwrap_or_default())
    }

    /// `BehaviorResult`: the current action finished (succeeded or failed).
    pub fn behavior_result(&mut self, who: i32, success: bool) -> Result<(), String> {
        let inst = self.instance(who)?;
        if let Some(n) = inst.running {
            inst.result = Some((n, success));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ENEMY: &str = "selector\n  sequence\n    if dist < 10\n    do chase\n  do patrol\n";

    fn action(t: &Tree, i: &mut Instance) -> String {
        t.update(i).map(|n| t.nodes[n].text.clone()).unwrap_or_default()
    }

    #[test]
    fn higher_priority_branches_take_over() {
        let t = Tree::parse("enemy", ENEMY).unwrap();
        let mut i = Instance::default();
        i.params.insert("dist".into(), 50.0);
        assert_eq!(action(&t, &mut i), "patrol");
        i.params.insert("dist".into(), 5.0);
        assert_eq!(action(&t, &mut i), "chase");
        i.params.insert("dist".into(), 20.0);
        assert_eq!(action(&t, &mut i), "patrol");
    }

    #[test]
    fn sequences_advance_on_success() {
        let t = Tree::parse("t", "sequence\n  do walk\n  do wait ; comment\n").unwrap();
        let mut i = Instance::default();
        assert_eq!(action(&t, &mut i), "walk");
        assert_eq!(action(&t, &mut i), "walk");
        i.result = Some((i.running.unwrap(), true));
        assert_eq!(action(&t, &mut i), "wait");
        i.result = Some((i.running.unwrap(), true));
        // finished: starts again
        assert_eq!(action(&t, &mut i), "walk");
        // a failure ends the sequence and it starts again too
        i.result = Some((i.running.unwrap(), false));
        assert_eq!(action(&t, &mut i), "walk");
    }

    #[test]
    fn conditions_compare_params() {
        let mut p = HashMap::new();
        p.insert("health".into(), 3.0);
        p.insert("max".into(), 10.0);
        assert!(parse_condition("health < max").unwrap().holds(&p));
        assert!(parse_condition("health >= 3").unwrap().holds(&p));
        assert!(parse_condition("health <> 4").unwrap().holds(&p));
        assert!(parse_condition("health").unwrap().holds(&p));
        assert!(!parse_condition("armed").unwrap().holds(&p));
        assert!(parse_condition("a < (b").is_err());
        let t = Tree::parse("t", "selector\n  sequence\n    invert\n      if armed\n    do flee\n  do fight").unwrap();
        let mut i = Instance { params: p, ..Default::default() };
        assert_eq!(action(&t, &mut i), "flee");
        i.params.insert("armed".into(), 1.0);
        assert_eq!(action(&t, &mut i), "fight");
        assert!(Tree::parse("t", "jump").is_err());
    }
}
