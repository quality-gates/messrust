//! Load phpmd-format rulesets and apply CLI filters.

use std::cell::RefCell;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::rc::Rc;

#[cfg(test)]
use std::cell::Cell;

use crate::analyze::RuleKind;

#[cfg(test)]
thread_local! {
    static BLOCKER_STATUS_CALLS: Cell<usize> = const { Cell::new(0) };
    static RULE_STATUS_CALLS: Cell<usize> = const { Cell::new(0) };
    static INTERSECTION_VISITS: Cell<usize> = const { Cell::new(0) };
}

/// A rule selected from a ruleset after load and filter.
#[derive(Clone, Debug)]
pub struct LoadedRule {
    pub name: String,
    pub ruleset_name: String,
    /// Priority 1 (highest) through 5.
    pub priority: u8,
    pub properties: BTreeMap<String, String>,
    pub kind: RuleKind,
    /// phpmd-style message template with `{0}` placeholders.
    pub message: String,
}

pub struct LoadOptions {
    pub min_priority: u8,
    pub max_priority: u8,
}

impl Default for LoadOptions {
    fn default() -> Self {
        Self {
            min_priority: 0,
            max_priority: 1,
        }
    }
}

#[derive(Clone)]
struct XmlRule {
    name: String,
    class: String,
    message: String,
    ref_path: String,
    priority: Option<u8>,
    properties: BTreeMap<String, String>,
    excludes: Vec<String>,
}

#[derive(Clone)]
struct XmlRuleset {
    name: String,
    rules: Vec<XmlRule>,
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct ExpansionKey(String, String);

#[derive(Clone, Hash, Eq, PartialEq)]
enum BlockCondition {
    Excluded,
    Priority { fallback: Option<u8> },
}

#[derive(Clone, Hash, Eq, PartialEq)]
struct BlockedRule {
    name: Rc<str>,
    condition: BlockCondition,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum BoundaryKind {
    Named,
    Full,
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct OverrideBoundary {
    kind: BoundaryKind,
    priority: Option<u8>,
    excludes: Vec<String>,
}

enum BlockerKind {
    Empty,
    Rules(Vec<BlockedRule>),
    All(Vec<ExpansionResult>),
    Boundary {
        boundary: OverrideBoundary,
        child: ExpansionResult,
    },
}

struct BlockerSummary {
    kind: BlockerKind,
    rule_names: Rc<NameTrie>,
    exclusion_names: Rc<NameTrie>,
    has_priority: bool,
    seen_projections: RefCell<HashMap<u64, u64>>,
    exclusion_projections: RefCell<HashMap<u64, u64>>,
    blocked_contexts: RefCell<HashSet<(u64, PriorityContext, u64)>>,
}

#[derive(Default)]
struct NameTrie {
    terminal: bool,
    children: HashMap<u8, Rc<NameTrie>>,
}

impl Drop for NameTrie {
    fn drop(&mut self) {
        let mut pending: Vec<_> = std::mem::take(&mut self.children).into_values().collect();
        while let Some(mut node) = pending.pop() {
            if let Some(node) = Rc::get_mut(&mut node) {
                pending.extend(std::mem::take(&mut node.children).into_values());
            }
        }
    }
}

type ExpansionResult = Rc<BlockerSummary>;

impl BlockerSummary {
    fn is_empty(&self) -> bool {
        matches!(&self.kind, BlockerKind::Empty)
    }
}

enum BlockStatus {
    Resolved,
    Blocked,
    Ready,
}

struct BlockerEvaluator<'a> {
    overrides: &'a mut ActiveOverrides,
    loaded: &'a LoadedRuleNames,
    opts: &'a LoadOptions,
}

struct LoadedRuleNames {
    names: HashSet<String>,
    context_names: Rc<ContextNameTrie>,
}

impl Default for LoadedRuleNames {
    fn default() -> Self {
        Self {
            names: HashSet::new(),
            context_names: ContextNameInterner::empty(),
        }
    }
}

#[derive(Clone, Eq, Hash, PartialEq)]
struct ExpansionNode(String, String);

struct ReferenceTarget {
    source_id: String,
    rule_name: String,
    source_name: String,
    source: Rc<XmlRuleset>,
}

struct ActiveOverrides {
    priority: Option<u8>,
    properties: BTreeMap<String, String>,
    excludes: HashMap<String, usize>,
    message: String,
    context_names: Rc<ContextNameTrie>,
    context_name_interner: ContextNameInterner,
}

struct ContextNameTrie {
    id: u64,
    terminal: bool,
    children: BTreeMap<u8, Rc<ContextNameTrie>>,
}

impl Drop for ContextNameTrie {
    fn drop(&mut self) {
        let mut pending: Vec<_> = std::mem::take(&mut self.children).into_values().collect();
        while let Some(mut node) = pending.pop() {
            if let Some(node) = Rc::get_mut(&mut node) {
                pending.extend(std::mem::take(&mut node.children).into_values());
            }
        }
    }
}

#[derive(Eq, Hash, PartialEq)]
struct ContextNameKey(bool, Vec<(u8, u64)>);

#[derive(Default)]
struct ContextNameInterner {
    next_id: u64,
    nodes: HashMap<ContextNameKey, Rc<ContextNameTrie>>,
    intersections: HashMap<(usize, u64), (Rc<NameTrie>, Rc<ContextNameTrie>)>,
}

struct NameUnionFrame {
    left: Rc<NameTrie>,
    terminal: bool,
    right_children: Vec<(u8, Rc<NameTrie>)>,
    next_child: usize,
    pending_byte: Option<u8>,
    children: HashMap<u8, Rc<NameTrie>>,
    changed: bool,
}

struct IntersectionFrame {
    key: (usize, u64),
    blocker: Rc<NameTrie>,
    terminal: bool,
    pairs: Vec<(u8, Rc<NameTrie>, Rc<ContextNameTrie>)>,
    next_pair: usize,
    pending_byte: Option<u8>,
    children: BTreeMap<u8, Rc<ContextNameTrie>>,
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum PriorityContext {
    Inherited,
    Accepted,
    Filtered,
}

struct OverrideCheckpoint {
    context_names: Rc<ContextNameTrie>,
    priority: Option<u8>,
    previous_properties: Option<BTreeMap<String, String>>,
    added_properties: Vec<String>,
    previous_excludes: Option<HashMap<String, usize>>,
    added_excludes: Vec<String>,
    previous_message: Option<String>,
    added_message: bool,
}

struct BlockCheckpoint {
    context_names: Rc<ContextNameTrie>,
    priority: Option<u8>,
    previous_excludes: Option<HashMap<String, usize>>,
    added_excludes: Vec<String>,
}

impl ContextNameInterner {
    fn empty() -> Rc<ContextNameTrie> {
        Rc::new(ContextNameTrie {
            id: 0,
            terminal: false,
            children: BTreeMap::new(),
        })
    }

    fn intern(
        &mut self,
        terminal: bool,
        children: BTreeMap<u8, Rc<ContextNameTrie>>,
    ) -> Rc<ContextNameTrie> {
        if !terminal && children.is_empty() {
            return Self::empty();
        }
        let key = ContextNameKey(
            terminal,
            children
                .iter()
                .map(|(byte, child)| (*byte, child.id))
                .collect(),
        );
        if let Some(node) = self.nodes.get(&key) {
            return Rc::clone(node);
        }
        self.next_id += 1;
        let node = Rc::new(ContextNameTrie {
            id: self.next_id,
            terminal,
            children,
        });
        self.nodes.insert(key, Rc::clone(&node));
        node
    }

    fn insert(&mut self, root: &Rc<ContextNameTrie>, name: &str) -> Rc<ContextNameTrie> {
        let mut path = Vec::with_capacity(name.len());
        let mut node = Rc::clone(root);
        for byte in name.bytes() {
            let child = node
                .children
                .get(&byte)
                .cloned()
                .unwrap_or_else(Self::empty);
            path.push((node, byte));
            node = child;
        }
        if node.terminal {
            return Rc::clone(root);
        }
        let mut inserted = self.intern(true, node.children.clone());
        for (parent, byte) in path.into_iter().rev() {
            let mut children = parent.children.clone();
            children.insert(byte, inserted);
            inserted = self.intern(parent.terminal, children);
        }
        inserted
    }

    fn intersection(
        &mut self,
        blockers: &Rc<NameTrie>,
        active: &Rc<ContextNameTrie>,
    ) -> Rc<ContextNameTrie> {
        let mut blocker = Rc::clone(blockers);
        let mut active = Rc::clone(active);
        let mut frames = Vec::new();
        'descend: loop {
            let key = (Rc::as_ptr(&blocker) as usize, active.id);
            let mut result = if let Some((_, cached)) = self.intersections.get(&key) {
                Rc::clone(cached)
            } else {
                #[cfg(test)]
                INTERSECTION_VISITS.with(|visits| visits.set(visits.get() + 1));
                let pairs = blocker
                    .children
                    .iter()
                    .filter_map(|(byte, blocker_child)| {
                        active.children.get(byte).map(|active_child| {
                            (*byte, Rc::clone(blocker_child), Rc::clone(active_child))
                        })
                    })
                    .collect();
                let mut frame = IntersectionFrame {
                    key,
                    blocker: Rc::clone(&blocker),
                    terminal: blocker.terminal && active.terminal,
                    pairs,
                    next_pair: 0,
                    pending_byte: None,
                    children: BTreeMap::new(),
                };
                if let Some((byte, next_blocker, next_active)) = frame.next() {
                    frame.pending_byte = Some(byte);
                    frames.push(frame);
                    blocker = next_blocker;
                    active = next_active;
                    continue 'descend;
                }
                let result = self.intern(frame.terminal, frame.children);
                self.intersections
                    .insert(key, (Rc::clone(&blocker), Rc::clone(&result)));
                result
            };

            loop {
                let Some(mut frame) = frames.pop() else {
                    return result;
                };
                let byte = frame.pending_byte.take().expect("intersection byte");
                if result.terminal || !result.children.is_empty() {
                    frame.children.insert(byte, result);
                }
                if let Some((byte, next_blocker, next_active)) = frame.next() {
                    frame.pending_byte = Some(byte);
                    frames.push(frame);
                    blocker = next_blocker;
                    active = next_active;
                    continue 'descend;
                }
                result = self.intern(frame.terminal, frame.children);
                self.intersections
                    .insert(frame.key, (Rc::clone(&frame.blocker), Rc::clone(&result)));
            }
        }
    }
}

impl IntersectionFrame {
    fn next(&mut self) -> Option<(u8, Rc<NameTrie>, Rc<ContextNameTrie>)> {
        let pair = self.pairs.get(self.next_pair).cloned();
        self.next_pair += usize::from(pair.is_some());
        pair
    }
}

impl Default for ActiveOverrides {
    fn default() -> Self {
        Self {
            priority: None,
            properties: BTreeMap::new(),
            excludes: HashMap::new(),
            message: String::new(),
            context_names: ContextNameInterner::empty(),
            context_name_interner: ContextNameInterner::default(),
        }
    }
}

impl ActiveOverrides {
    fn add_context_name(&mut self, name: &str) {
        let names = self.context_name_interner.insert(&self.context_names, name);
        self.context_names = names;
    }

    fn replace_context_names(&mut self, names: &[String]) {
        self.context_names = ContextNameInterner::empty();
        for name in names {
            self.add_context_name(name);
        }
    }

    fn push_named(&mut self, rule: &XmlRule) -> OverrideCheckpoint {
        let mut added_properties = Vec::new();
        for (name, value) in &rule.properties {
            if !self.properties.contains_key(name) {
                self.properties.insert(name.clone(), value.clone());
                added_properties.push(name.clone());
            }
        }
        let previous_excludes = std::mem::take(&mut self.excludes);
        let previous_context_names = Rc::clone(&self.context_names);
        self.replace_context_names(&rule.excludes);
        for name in &rule.excludes {
            *self.excludes.entry(name.clone()).or_default() += 1;
        }
        let added_message = self.message.is_empty() && !rule.message.is_empty();
        if added_message {
            self.message.clone_from(&rule.message);
        }
        let checkpoint = OverrideCheckpoint {
            context_names: previous_context_names,
            priority: self.priority,
            previous_properties: None,
            added_properties,
            previous_excludes: Some(previous_excludes),
            added_excludes: Vec::new(),
            previous_message: None,
            added_message,
        };
        self.priority = rule.priority.or(self.priority);
        checkpoint
    }

    fn push_full(&mut self, rule: &XmlRule) -> OverrideCheckpoint {
        let previous_properties = std::mem::take(&mut self.properties);
        let previous_context_names = Rc::clone(&self.context_names);
        self.properties.clone_from(&rule.properties);
        for name in &rule.excludes {
            if !self.excludes.contains_key(name) {
                self.add_context_name(name);
            }
            *self.excludes.entry(name.clone()).or_default() += 1;
        }
        let previous_message = std::mem::replace(&mut self.message, rule.message.clone());
        let checkpoint = OverrideCheckpoint {
            context_names: previous_context_names,
            priority: self.priority,
            previous_properties: Some(previous_properties),
            added_properties: Vec::new(),
            previous_excludes: None,
            added_excludes: rule.excludes.clone(),
            previous_message: Some(previous_message),
            added_message: false,
        };
        self.priority = rule.priority.or(self.priority);
        checkpoint
    }

    fn push_boundary(&mut self, rule: &XmlRule, kind: BoundaryKind) -> OverrideCheckpoint {
        match kind {
            BoundaryKind::Named => self.push_named(rule),
            BoundaryKind::Full => self.push_full(rule),
        }
    }

    fn pop(&mut self, checkpoint: OverrideCheckpoint) {
        self.context_names = checkpoint.context_names;
        self.priority = checkpoint.priority;
        if let Some(previous) = checkpoint.previous_properties {
            self.properties = previous;
        } else {
            for name in checkpoint.added_properties {
                self.properties.remove(&name);
            }
        }
        if let Some(previous) = checkpoint.previous_excludes {
            self.excludes = previous;
        } else {
            for name in checkpoint.added_excludes {
                let count = self.excludes.get_mut(&name).expect("active exclusion");
                *count -= 1;
                if *count == 0 {
                    self.excludes.remove(&name);
                }
            }
        }
        if let Some(previous) = checkpoint.previous_message {
            self.message = previous;
        } else if checkpoint.added_message {
            self.message.clear();
        }
    }

    fn push_block(&mut self, boundary: &OverrideBoundary) -> BlockCheckpoint {
        let previous_context_names = Rc::clone(&self.context_names);
        let checkpoint = match boundary.kind {
            BoundaryKind::Named => BlockCheckpoint {
                context_names: previous_context_names,
                priority: self.priority,
                previous_excludes: Some(std::mem::take(&mut self.excludes)),
                added_excludes: Vec::new(),
            },
            BoundaryKind::Full => BlockCheckpoint {
                context_names: previous_context_names,
                priority: self.priority,
                previous_excludes: None,
                added_excludes: boundary.excludes.clone(),
            },
        };
        if matches!(boundary.kind, BoundaryKind::Named) {
            self.replace_context_names(&boundary.excludes);
        }
        for name in &boundary.excludes {
            if matches!(boundary.kind, BoundaryKind::Full) && !self.excludes.contains_key(name) {
                self.add_context_name(name);
            }
            *self.excludes.entry(name.clone()).or_default() += 1;
        }
        self.priority = boundary.priority.or(self.priority);
        checkpoint
    }

    fn pop_block(&mut self, checkpoint: BlockCheckpoint) {
        self.context_names = checkpoint.context_names;
        self.priority = checkpoint.priority;
        if let Some(previous) = checkpoint.previous_excludes {
            self.excludes = previous;
            return;
        }
        for name in checkpoint.added_excludes {
            let count = self.excludes.get_mut(&name).expect("active exclusion");
            *count -= 1;
            if *count == 0 {
                self.excludes.remove(&name);
            }
        }
    }
}

fn empty_summary() -> ExpansionResult {
    Rc::new(BlockerSummary {
        kind: BlockerKind::Empty,
        rule_names: Rc::new(NameTrie::default()),
        exclusion_names: Rc::new(NameTrie::default()),
        has_priority: false,
        seen_projections: RefCell::new(HashMap::new()),
        exclusion_projections: RefCell::new(HashMap::new()),
        blocked_contexts: RefCell::new(HashSet::new()),
    })
}

fn name_trie_insert(root: &Rc<NameTrie>, name: &str) -> Rc<NameTrie> {
    let mut path = Vec::with_capacity(name.len());
    let mut node = Rc::clone(root);
    for byte in name.bytes() {
        let child = node
            .children
            .get(&byte)
            .cloned()
            .unwrap_or_else(|| Rc::new(NameTrie::default()));
        path.push((node, byte));
        node = child;
    }
    if node.terminal {
        return Rc::clone(root);
    }
    let mut inserted = Rc::new(NameTrie {
        terminal: true,
        children: node.children.clone(),
    });
    for (parent, byte) in path.into_iter().rev() {
        let mut children = parent.children.clone();
        children.insert(byte, inserted);
        inserted = Rc::new(NameTrie {
            terminal: parent.terminal,
            children,
        });
    }
    inserted
}

fn name_trie_union(left: &Rc<NameTrie>, right: &Rc<NameTrie>) -> Rc<NameTrie> {
    let mut current_left = Rc::clone(left);
    let mut current_right = Rc::clone(right);
    let mut frames = Vec::new();
    'descend: loop {
        let mut result =
            if let Some(shared) = name_trie_union_shortcut(&current_left, &current_right) {
                shared
            } else {
                let mut frame = NameUnionFrame::new(&current_left, &current_right);
                if let Some((byte, next_left, next_right)) = frame.next() {
                    frame.pending_byte = Some(byte);
                    frames.push(frame);
                    current_left = next_left;
                    current_right = next_right;
                    continue 'descend;
                }
                frame.finish()
            };

        loop {
            let Some(mut frame) = frames.pop() else {
                return result;
            };
            let byte = frame.pending_byte.take().expect("union byte");
            let left_child = frame.left.children.get(&byte).expect("left union child");
            if !Rc::ptr_eq(left_child, &result) {
                frame.children.insert(byte, result);
                frame.changed = true;
            }
            if let Some((byte, next_left, next_right)) = frame.next() {
                frame.pending_byte = Some(byte);
                frames.push(frame);
                current_left = next_left;
                current_right = next_right;
                continue 'descend;
            }
            result = frame.finish();
        }
    }
}

fn name_trie_union_shortcut(left: &Rc<NameTrie>, right: &Rc<NameTrie>) -> Option<Rc<NameTrie>> {
    if Rc::ptr_eq(left, right) {
        return Some(Rc::clone(left));
    }
    if !right.terminal && right.children.is_empty() {
        return Some(Rc::clone(left));
    }
    if !left.terminal && left.children.is_empty() {
        return Some(Rc::clone(right));
    }
    None
}

impl NameUnionFrame {
    fn new(left: &Rc<NameTrie>, right: &Rc<NameTrie>) -> Self {
        let mut children = left.children.clone();
        let mut changed = !left.terminal && right.terminal;
        let mut right_children = Vec::new();
        for (byte, right_child) in &right.children {
            if left.children.contains_key(byte) {
                right_children.push((*byte, Rc::clone(right_child)));
            } else {
                children.insert(*byte, Rc::clone(right_child));
                changed = true;
            }
        }
        Self {
            left: Rc::clone(left),
            terminal: left.terminal || right.terminal,
            right_children,
            next_child: 0,
            pending_byte: None,
            children,
            changed,
        }
    }

    fn next(&mut self) -> Option<(u8, Rc<NameTrie>, Rc<NameTrie>)> {
        let (byte, right) = self.right_children.get(self.next_child)?.clone();
        self.next_child += 1;
        let left = self.left.children.get(&byte).expect("left union child");
        Some((byte, Rc::clone(left), right))
    }

    fn finish(self) -> Rc<NameTrie> {
        if !self.changed {
            return self.left;
        }
        Rc::new(NameTrie {
            terminal: self.terminal,
            children: self.children,
        })
    }
}

fn name_trie_contains(root: &NameTrie, name: &str) -> bool {
    let mut node = root;
    for byte in name.bytes() {
        let Some(child) = node.children.get(&byte) else {
            return false;
        };
        node = child;
    }
    node.terminal
}

fn all_summary(children: Vec<ExpansionResult>) -> ExpansionResult {
    let mut children: Vec<_> = children
        .into_iter()
        .filter(|child| !matches!(&child.kind, BlockerKind::Empty))
        .collect();
    let mut addresses = HashSet::new();
    children.retain(|child| addresses.insert(Rc::as_ptr(child)));
    match children.len() {
        0 => empty_summary(),
        1 => children.pop().expect("one blocker summary"),
        _ => {
            let rule_names = children
                .iter()
                .fold(Rc::new(NameTrie::default()), |names, child| {
                    name_trie_union(&names, &child.rule_names)
                });
            let exclusion_names = children
                .iter()
                .fold(Rc::new(NameTrie::default()), |names, child| {
                    name_trie_union(&names, &child.exclusion_names)
                });
            let has_priority = children.iter().any(|child| child.has_priority);
            Rc::new(BlockerSummary {
                kind: BlockerKind::All(children),
                rule_names,
                exclusion_names,
                has_priority,
                seen_projections: RefCell::new(HashMap::new()),
                exclusion_projections: RefCell::new(HashMap::new()),
                blocked_contexts: RefCell::new(HashSet::new()),
            })
        }
    }
}

fn rules_summary(rules: Vec<BlockedRule>) -> ExpansionResult {
    if rules.is_empty() {
        return empty_summary();
    }
    let rule_names = rules
        .iter()
        .fold(Rc::new(NameTrie::default()), |names, rule| {
            name_trie_insert(&names, &rule.name)
        });
    let exclusion_names = rules
        .iter()
        .filter(|rule| matches!(rule.condition, BlockCondition::Excluded))
        .fold(Rc::new(NameTrie::default()), |names, rule| {
            name_trie_insert(&names, &rule.name)
        });
    let has_priority = rules
        .iter()
        .any(|rule| matches!(rule.condition, BlockCondition::Priority { .. }));
    Rc::new(BlockerSummary {
        kind: BlockerKind::Rules(rules),
        rule_names,
        exclusion_names,
        has_priority,
        seen_projections: RefCell::new(HashMap::new()),
        exclusion_projections: RefCell::new(HashMap::new()),
        blocked_contexts: RefCell::new(HashSet::new()),
    })
}

fn boundary_affects(child: &ExpansionResult, rule: &XmlRule, kind: BoundaryKind) -> bool {
    if matches!(&child.kind, BlockerKind::Empty) {
        return false;
    }
    let affects_exclusions = match kind {
        BoundaryKind::Named => {
            child.exclusion_names.terminal || !child.exclusion_names.children.is_empty()
        }
        BoundaryKind::Full => rule
            .excludes
            .iter()
            .any(|name| name_trie_contains(&child.exclusion_names, name)),
    };
    affects_exclusions || (child.has_priority && rule.priority.is_some())
}

fn boundary_summary(child: ExpansionResult, rule: &XmlRule, kind: BoundaryKind) -> ExpansionResult {
    if !boundary_affects(&child, rule, kind) {
        return child;
    }
    Rc::new(BlockerSummary {
        rule_names: Rc::clone(&child.rule_names),
        exclusion_names: Rc::clone(&child.exclusion_names),
        has_priority: child.has_priority,
        seen_projections: RefCell::new(HashMap::new()),
        exclusion_projections: RefCell::new(HashMap::new()),
        blocked_contexts: RefCell::new(HashSet::new()),
        kind: BlockerKind::Boundary {
            boundary: OverrideBoundary {
                kind,
                priority: rule.priority,
                excludes: rule
                    .excludes
                    .iter()
                    .filter(|name| name_trie_contains(&child.exclusion_names, name))
                    .cloned()
                    .collect(),
            },
            child,
        },
    })
}

impl BlockerEvaluator<'_> {
    fn exclusion_context(&mut self, summary: &ExpansionResult) -> u64 {
        let active_id = self.overrides.context_names.id;
        let cached_projection = summary
            .exclusion_projections
            .borrow()
            .get(&active_id)
            .copied();
        if let Some(projected) = cached_projection {
            return projected;
        }
        let projected = self
            .overrides
            .context_name_interner
            .intersection(&summary.exclusion_names, &self.overrides.context_names);
        let projected_id = projected.id;
        summary
            .exclusion_projections
            .borrow_mut()
            .insert(active_id, projected_id);
        projected_id
    }

    fn priority_context(&self, summary: &ExpansionResult) -> PriorityContext {
        match (summary.has_priority, self.overrides.priority) {
            (false, _) | (true, None) => PriorityContext::Inherited,
            (true, Some(priority))
                if (self.opts.min_priority > 0 && priority > self.opts.min_priority)
                    || (self.opts.max_priority > 0 && priority < self.opts.max_priority) =>
            {
                PriorityContext::Filtered
            }
            (true, Some(_)) => PriorityContext::Accepted,
        }
    }

    fn seen_context(&mut self, summary: &ExpansionResult) -> u64 {
        let seen_id = self.loaded.context_names.id;
        let cached_seen_projection = summary.seen_projections.borrow().get(&seen_id).copied();
        if let Some(projected) = cached_seen_projection {
            return projected;
        }
        let projected = self
            .overrides
            .context_name_interner
            .intersection(&summary.rule_names, &self.loaded.context_names);
        let projected_id = projected.id;
        summary
            .seen_projections
            .borrow_mut()
            .insert(seen_id, projected_id);
        projected_id
    }

    fn context(&mut self, summary: &ExpansionResult) -> (u64, PriorityContext, u64) {
        (
            self.exclusion_context(summary),
            self.priority_context(summary),
            self.seen_context(summary),
        )
    }

    fn remember_blocked(&mut self, summary: &ExpansionResult) {
        let context = self.context(summary);
        summary.blocked_contexts.borrow_mut().insert(context);
    }

    fn rule_status(&self, rule: &BlockedRule) -> BlockStatus {
        #[cfg(test)]
        RULE_STATUS_CALLS.with(|calls| calls.set(calls.get() + 1));
        if self.loaded.names.contains(rule.name.as_ref()) {
            return BlockStatus::Resolved;
        }
        let blocked = match rule.condition {
            BlockCondition::Excluded => self.overrides.excludes.contains_key(rule.name.as_ref()),
            BlockCondition::Priority { fallback } => {
                let priority = self.overrides.priority.or(fallback).unwrap_or(3);
                (self.opts.min_priority > 0 && priority > self.opts.min_priority)
                    || (self.opts.max_priority > 0 && priority < self.opts.max_priority)
            }
        };
        if blocked {
            BlockStatus::Blocked
        } else {
            BlockStatus::Ready
        }
    }

    fn status(&mut self, summary: &ExpansionResult) -> BlockStatus {
        #[cfg(test)]
        BLOCKER_STATUS_CALLS.with(|calls| calls.set(calls.get() + 1));
        let context = self.context(summary);
        if summary.blocked_contexts.borrow().contains(&context) {
            return BlockStatus::Blocked;
        }
        let status = match &summary.kind {
            BlockerKind::Empty => BlockStatus::Resolved,
            BlockerKind::Rules(rules) => self.rules_status(rules),
            BlockerKind::All(children) => self.all_status(children),
            BlockerKind::Boundary { boundary, child } => {
                let checkpoint = self.overrides.push_block(boundary);
                let status = self.status(child);
                self.overrides.pop_block(checkpoint);
                status
            }
        };
        if matches!(status, BlockStatus::Blocked) {
            summary.blocked_contexts.borrow_mut().insert(context);
        }
        status
    }

    fn rules_status(&self, rules: &[BlockedRule]) -> BlockStatus {
        let mut status = BlockStatus::Resolved;
        for rule in rules {
            match self.rule_status(rule) {
                BlockStatus::Ready => return BlockStatus::Ready,
                BlockStatus::Blocked => status = BlockStatus::Blocked,
                BlockStatus::Resolved => {}
            }
        }
        status
    }

    fn all_status(&mut self, children: &[ExpansionResult]) -> BlockStatus {
        let mut status = BlockStatus::Resolved;
        for child in children {
            match self.status(child) {
                BlockStatus::Ready => return BlockStatus::Ready,
                BlockStatus::Blocked => status = BlockStatus::Blocked,
                BlockStatus::Resolved => {}
            }
        }
        status
    }
}

#[derive(Default)]
struct ExpansionState {
    parsed: HashMap<String, Rc<XmlRuleset>>,
    active: Vec<ExpansionNode>,
    active_indices: HashMap<ExpansionNode, usize>,
    completed: HashSet<ExpansionKey>,
    incomplete: HashMap<ExpansionKey, ExpansionResult>,
}

struct RulesetLoader<'a> {
    expansion: ExpansionState,
    overrides: ActiveOverrides,
    loaded: LoadedRuleNames,
    opts: &'a LoadOptions,
    warn: &'a mut dyn FnMut(String),
}

fn expansion_key(source_id: &str, rule_name: &str) -> ExpansionKey {
    ExpansionKey(source_id.to_string(), rule_name.to_string())
}

impl<'a> RulesetLoader<'a> {
    fn new(opts: &'a LoadOptions, warn: &'a mut dyn FnMut(String)) -> Self {
        Self {
            expansion: ExpansionState::default(),
            overrides: ActiveOverrides::default(),
            loaded: LoadedRuleNames::default(),
            opts,
            warn,
        }
    }

    fn source(&mut self, ident: &str) -> Result<(String, String, Rc<XmlRuleset>), String> {
        let source_id = stable_ruleset_id(ident)?;
        if let Some(parsed) = self.expansion.parsed.get(&source_id) {
            return Ok((source_id, ruleset_display_name(ident), Rc::clone(parsed)));
        }
        let (xml, display_name) = read_ruleset(ident)?;
        let parsed = Rc::new(parse_ruleset(&xml)?);
        self.expansion
            .parsed
            .insert(source_id.clone(), Rc::clone(&parsed));
        Ok((source_id, display_name, parsed))
    }

    fn load_one(&mut self, ident: &str, out: &mut Vec<LoadedRule>) -> Result<(), String> {
        let (source_id, display_name, source) = self.source(ident)?;
        let set_name = if source.name.is_empty() {
            display_name
        } else {
            source.name.clone()
        };
        self.expand_source(out, &source_id, &source, &set_name, "")
            .map(|_| ())
    }

    fn cached_result(&mut self, key: &ExpansionKey) -> Option<ExpansionResult> {
        if self.expansion.completed.contains(key) {
            return Some(empty_summary());
        }
        let blocked = self.expansion.incomplete.get(key)?.clone();
        let status = BlockerEvaluator {
            overrides: &mut self.overrides,
            loaded: &self.loaded,
            opts: self.opts,
        }
        .status(&blocked);
        match status {
            BlockStatus::Blocked => Some(blocked),
            BlockStatus::Ready => None,
            BlockStatus::Resolved => {
                self.expansion.incomplete.remove(key);
                self.expansion.completed.insert(key.clone());
                Some(empty_summary())
            }
        }
    }

    fn expand_source(
        &mut self,
        out: &mut Vec<LoadedRule>,
        source_id: &str,
        source: &XmlRuleset,
        source_name: &str,
        rule_name: &str,
    ) -> Result<ExpansionResult, String> {
        let node = ExpansionNode(source_id.to_string(), rule_name.to_string());
        if let Some(start) = self.expansion.active_indices.get(&node).copied() {
            let mut chain: Vec<String> = self.expansion.active[start..]
                .iter()
                .map(|active| active.0.clone())
                .collect();
            chain.push(node.0.clone());
            return Err(format!("ruleset reference cycle: {}", chain.join(" -> ")));
        }

        let key = expansion_key(source_id, rule_name);
        if let Some(cached) = self.cached_result(&key) {
            return Ok(cached);
        }

        self.expansion
            .active_indices
            .insert(node.clone(), self.expansion.active.len());
        self.expansion.active.push(node);
        let result = if rule_name.is_empty() {
            self.add_ruleset_rules(out, source, source_name)
        } else {
            self.add_named_rule(out, source, source_name, rule_name)
        };
        let node = self
            .expansion
            .active
            .pop()
            .expect("active ruleset expansion");
        self.expansion.active_indices.remove(&node);
        if let Ok(blocked) = &result {
            if blocked.is_empty() {
                self.expansion.incomplete.remove(&key);
                self.expansion.completed.insert(key);
            } else {
                BlockerEvaluator {
                    overrides: &mut self.overrides,
                    loaded: &self.loaded,
                    opts: self.opts,
                }
                .remember_blocked(blocked);
                self.expansion.incomplete.insert(key, Rc::clone(blocked));
            }
        }
        result
    }

    fn add_ref_with_boundary(
        &mut self,
        out: &mut Vec<LoadedRule>,
        rule: &XmlRule,
        named: bool,
    ) -> Result<ExpansionResult, String> {
        let Some(target) = self.reference_target(rule)? else {
            return Ok(empty_summary());
        };
        let key = expansion_key(&target.source_id, &target.rule_name);
        let kind = if named {
            BoundaryKind::Named
        } else {
            BoundaryKind::Full
        };
        if let Some(blocked) = self.cached_without_boundary(&key, rule, named) {
            return Ok(blocked);
        }

        let checkpoint = self.overrides.push_boundary(rule, kind);
        let result = self.expand_source(
            out,
            &target.source_id,
            &target.source,
            &target.source_name,
            &target.rule_name,
        );
        self.overrides.pop(checkpoint);
        self.finish_boundary(result?, rule, named)
    }

    fn reference_target(&mut self, rule: &XmlRule) -> Result<Option<ReferenceTarget>, String> {
        let (base, rule_name) = split_ref(&rule.ref_path);
        let (source_id, display_name, source) = match self.source(&base) {
            Ok(source) => source,
            Err(error) if is_resolvable(&base) => return Err(error),
            Err(_) => {
                (self.warn)(format!("Cannot resolve ref: {}", rule.ref_path));
                return Ok(None);
            }
        };
        let source_name = if source.name.is_empty() {
            display_name
        } else {
            source.name.clone()
        };
        Ok(Some(ReferenceTarget {
            source_id,
            rule_name,
            source_name,
            source,
        }))
    }

    fn cached_without_boundary(
        &mut self,
        key: &ExpansionKey,
        rule: &XmlRule,
        named: bool,
    ) -> Option<ExpansionResult> {
        let kind = if named {
            BoundaryKind::Named
        } else {
            BoundaryKind::Full
        };
        let blocked = self.expansion.incomplete.get(key)?.clone();
        if boundary_affects(&blocked, rule, kind) {
            return None;
        }
        self.cached_result(key)
    }

    fn finish_boundary(
        &mut self,
        child: ExpansionResult,
        rule: &XmlRule,
        named: bool,
    ) -> Result<ExpansionResult, String> {
        let kind = if named {
            BoundaryKind::Named
        } else {
            BoundaryKind::Full
        };
        let blocked = boundary_summary(Rc::clone(&child), rule, kind);
        if Rc::ptr_eq(&blocked, &child) {
            BlockerEvaluator {
                overrides: &mut self.overrides,
                loaded: &self.loaded,
                opts: self.opts,
            }
            .remember_blocked(&blocked);
        }
        Ok(blocked)
    }

    fn add_named_rule(
        &mut self,
        out: &mut Vec<LoadedRule>,
        source: &XmlRuleset,
        source_name: &str,
        rule_name: &str,
    ) -> Result<ExpansionResult, String> {
        let Some(source_rule) = find_source_rule(source, rule_name) else {
            return Ok(empty_summary());
        };
        if !source_rule.ref_path.is_empty() {
            return self.add_ref_with_boundary(out, source_rule, true);
        }
        Ok(self
            .emit_rule(out, source_name, source_rule, true)
            .map_or_else(empty_summary, |rule| rules_summary(vec![rule])))
    }

    fn add_ruleset_rules(
        &mut self,
        out: &mut Vec<LoadedRule>,
        source: &XmlRuleset,
        source_name: &str,
    ) -> Result<ExpansionResult, String> {
        let mut children = Vec::new();
        let mut local_rules = Vec::new();
        for source_rule in &source.rules {
            let name = rule_name_or_ref(source_rule);
            if !name.is_empty() && self.overrides.excludes.contains_key(name) {
                if !self.loaded.names.contains(name) {
                    local_rules.push(BlockedRule {
                        name: Rc::from(name),
                        condition: BlockCondition::Excluded,
                    });
                }
                continue;
            }
            if !source_rule.ref_path.is_empty() {
                children.push(self.add_ref_with_boundary(out, source_rule, false)?);
            } else if !source_rule.class.is_empty() {
                if let Some(rule) = self.emit_rule(out, source_name, source_rule, false) {
                    local_rules.push(rule);
                }
            }
        }
        children.push(rules_summary(local_rules));
        Ok(all_summary(children))
    }

    fn emit_rule(
        &mut self,
        out: &mut Vec<LoadedRule>,
        set_name: &str,
        def: &XmlRule,
        use_overrides: bool,
    ) -> Option<BlockedRule> {
        if self.loaded.names.contains(&def.name) {
            return None;
        }
        let overrides = use_overrides.then_some(&self.overrides);
        match build_rule(set_name, def, overrides, self.opts, self.warn) {
            BuildRuleResult::Loaded(rule) => {
                self.loaded.names.insert(rule.name.clone());
                self.loaded.context_names = self
                    .overrides
                    .context_name_interner
                    .insert(&self.loaded.context_names, &rule.name);
                out.push(rule);
                None
            }
            BuildRuleResult::Filtered if use_overrides => Some(BlockedRule {
                name: Rc::from(def.name.as_str()),
                condition: BlockCondition::Priority {
                    fallback: def.priority,
                },
            }),
            BuildRuleResult::Filtered | BuildRuleResult::Unsupported => None,
        }
    }
}

/// Load comma-separated ruleset names and/or XML paths, then apply filters.
pub fn load_and_filter(
    specs: &[String],
    only: &[String],
    disable: &[String],
    opts: &LoadOptions,
    warn: &mut dyn FnMut(String),
) -> Result<Vec<LoadedRule>, String> {
    let mut rules = Vec::new();
    let mut loader = RulesetLoader::new(opts, warn);
    for spec in specs {
        loader.load_one(spec, &mut rules)?;
    }
    if rules.is_empty() && specs.is_empty() {
        return Err("no rulesets specified".to_string());
    }
    apply_name_filters(&mut rules, only, disable)?;
    Ok(rules)
}

fn apply_name_filters(
    rules: &mut Vec<LoadedRule>,
    only: &[String],
    disable: &[String],
) -> Result<(), String> {
    let loaded: HashSet<&str> = rules.iter().map(|r| r.name.as_str()).collect();
    for name in only.iter().chain(disable.iter()) {
        if !loaded.contains(name.as_str()) {
            return Err(format!(
                "rule '{name}' is not present in the loaded rulesets"
            ));
        }
    }
    if !only.is_empty() {
        let keep: HashSet<&str> = only.iter().map(String::as_str).collect();
        rules.retain(|r| keep.contains(r.name.as_str()));
    }
    if !disable.is_empty() {
        let drop: HashSet<&str> = disable.iter().map(String::as_str).collect();
        rules.retain(|r| !drop.contains(r.name.as_str()));
    }
    Ok(())
}

fn rule_name_or_ref(rule: &XmlRule) -> &str {
    if !rule.name.is_empty() {
        &rule.name
    } else if let Some((_, name)) = rule.ref_path.split_once('/') {
        name
    } else {
        &rule.ref_path
    }
}

fn find_source_rule<'a>(source: &'a XmlRuleset, rule_name: &str) -> Option<&'a XmlRule> {
    source
        .rules
        .iter()
        .find(|rule| rule_name_or_ref(rule) == rule_name)
}

enum BuildRuleResult {
    Loaded(LoadedRule),
    Filtered,
    Unsupported,
}

fn build_rule(
    set_name: &str,
    def: &XmlRule,
    overrides: Option<&ActiveOverrides>,
    opts: &LoadOptions,
    warn: &mut dyn FnMut(String),
) -> BuildRuleResult {
    let priority = overrides
        .and_then(|active| active.priority)
        .or(def.priority)
        .unwrap_or(3);
    if opts.min_priority > 0 && priority > opts.min_priority {
        return BuildRuleResult::Filtered;
    }
    if opts.max_priority > 0 && priority < opts.max_priority {
        return BuildRuleResult::Filtered;
    }
    let mut properties = def.properties.clone();
    if let Some(overrides) = overrides {
        for (name, value) in &overrides.properties {
            properties.insert(name.clone(), value.clone());
        }
    }
    let Some(kind) = resolve_kind(&def.class) else {
        warn(format!(
            "Skipping unimplemented rule {} ({})",
            def.name, def.class
        ));
        return BuildRuleResult::Unsupported;
    };
    let message = overrides
        .map(|active| active.message.as_str())
        .filter(|message| !message.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| def.message.clone());
    BuildRuleResult::Loaded(LoadedRule {
        name: def.name.clone(),
        ruleset_name: set_name.to_string(),
        priority,
        properties,
        kind,
        message,
    })
}

fn resolve_kind(class: &str) -> Option<RuleKind> {
    RULE_KINDS
        .iter()
        .find(|(known_class, _)| *known_class == class)
        .map(|(_, kind)| *kind)
}

const RULE_KINDS: &[(&str, RuleKind)] = &[
    (
        "PHPMD\\Rule\\CyclomaticComplexity",
        RuleKind::CyclomaticComplexity,
    ),
    (
        "PHPMD\\Rule\\Design\\NpathComplexity",
        RuleKind::NPathComplexity,
    ),
    (
        "PHPMD\\Rule\\Design\\LongMethod",
        RuleKind::ExcessiveMethodLength,
    ),
    (
        "PHPMD\\Rule\\Design\\LongClass",
        RuleKind::ExcessiveClassLength,
    ),
    (
        "PHPMD\\Rule\\Design\\LongParameterList",
        RuleKind::ExcessiveParameterList,
    ),
    (
        "PHPMD\\Rule\\ExcessivePublicCount",
        RuleKind::ExcessivePublicCount,
    ),
    (
        "PHPMD\\Rule\\Design\\TooManyFields",
        RuleKind::TooManyFields,
    ),
    (
        "PHPMD\\Rule\\Design\\TooManyMethods",
        RuleKind::TooManyMethods,
    ),
    (
        "PHPMD\\Rule\\Design\\TooManyPublicMethods",
        RuleKind::TooManyPublicMethods,
    ),
    (
        "PHPMD\\Rule\\Design\\WeightedMethodCount",
        RuleKind::ExcessiveClassComplexity,
    ),
    (
        "PHPMD\\Rule\\Naming\\ShortClassName",
        RuleKind::ShortClassName,
    ),
    (
        "PHPMD\\Rule\\Naming\\LongClassName",
        RuleKind::LongClassName,
    ),
    (
        "PHPMD\\Rule\\Naming\\ShortVariable",
        RuleKind::ShortVariable,
    ),
    ("PHPMD\\Rule\\Naming\\LongVariable", RuleKind::LongVariable),
    (
        "PHPMD\\Rule\\Naming\\ShortMethodName",
        RuleKind::ShortMethodName,
    ),
    (
        "PHPMD\\Rule\\Naming\\ConstantNamingConventions",
        RuleKind::ConstantNamingConventions,
    ),
    (
        "PHPMD\\Rule\\Naming\\BooleanGetMethodName",
        RuleKind::BooleanGetMethodName,
    ),
    (
        "PHPMD\\Rule\\UnusedPrivateField",
        RuleKind::UnusedPrivateField,
    ),
    (
        "PHPMD\\Rule\\UnusedLocalVariable",
        RuleKind::UnusedLocalVariable,
    ),
    (
        "PHPMD\\Rule\\UnusedPrivateMethod",
        RuleKind::UnusedPrivateMethod,
    ),
    (
        "PHPMD\\Rule\\UnusedFormalParameter",
        RuleKind::UnusedFormalParameter,
    ),
    (
        "PHPMD\\Rule\\CleanCode\\BooleanArgumentFlag",
        RuleKind::BooleanArgumentFlag,
    ),
    (
        "PHPMD\\Rule\\CleanCode\\ElseExpression",
        RuleKind::ElseExpression,
    ),
    (
        "PHPMD\\Rule\\CleanCode\\IfStatementAssignment",
        RuleKind::IfStatementAssignment,
    ),
    (
        "PHPMD\\Rule\\CleanCode\\DuplicatedArrayKey",
        RuleKind::DuplicatedArrayKey,
    ),
    (
        "PHPMD\\Rule\\CleanCode\\StaticAccess",
        RuleKind::StaticAccess,
    ),
    (
        "PHPMD\\Rule\\Design\\ExitExpression",
        RuleKind::ExitExpression,
    ),
    (
        "PHPMD\\Rule\\Design\\GotoStatement",
        RuleKind::GotoStatement,
    ),
    (
        "PHPMD\\Rule\\Design\\CountInLoopExpression",
        RuleKind::CountInLoopExpression,
    ),
    (
        "PHPMD\\Rule\\Design\\DevelopmentCodeFragment",
        RuleKind::DevelopmentCodeFragment,
    ),
    (
        "PHPMD\\Rule\\Design\\EmptyCatchBlock",
        RuleKind::EmptyCatchBlock,
    ),
    (
        "PHPMD\\Rule\\Design\\CouplingBetweenObjects",
        RuleKind::CouplingBetweenObjects,
    ),
    (
        "PHPMD\\Rule\\Design\\GlobalVariable",
        RuleKind::GlobalVariable,
    ),
    (
        "PHPMD\\Rule\\Design\\LackOfCohesionOfMethods",
        RuleKind::LackOfCohesionOfMethods,
    ),
    (
        "PHPMD\\Rule\\Controversial\\CamelCaseClassName",
        RuleKind::CamelCaseClassName,
    ),
    (
        "PHPMD\\Rule\\Controversial\\CamelCaseMethodName",
        RuleKind::CamelCaseMethodName,
    ),
    (
        "PHPMD\\Rule\\Controversial\\CamelCasePropertyName",
        RuleKind::CamelCasePropertyName,
    ),
    (
        "PHPMD\\Rule\\Controversial\\CamelCaseParameterName",
        RuleKind::CamelCaseParameterName,
    ),
    (
        "PHPMD\\Rule\\Controversial\\CamelCaseVariableName",
        RuleKind::CamelCaseVariableName,
    ),
];

fn read_ruleset(ident: &str) -> Result<(String, String), String> {
    let path = PathBuf::from(ident);
    if path.is_file() {
        let xml = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        let name = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or(ident)
            .to_string();
        return Ok((xml, name));
    }
    if let Some((xml, name)) = builtin_xml(ident) {
        return Ok((xml.to_string(), name.to_string()));
    }
    Err(format!("unknown ruleset or file: {ident}"))
}

fn stable_ruleset_id(ident: &str) -> Result<String, String> {
    let path = PathBuf::from(ident);
    if path.is_file() {
        return fs::canonicalize(&path)
            .map(|path| path.to_string_lossy().into_owned())
            .map_err(|error| format!("{}: {error}", path.display()));
    }
    if let Some(key) = normalize_builtin_key(ident) {
        return Ok(format!("builtin:{key}"));
    }
    Err(format!("unknown ruleset or file: {ident}"))
}

fn ruleset_display_name(ident: &str) -> String {
    let path = Path::new(ident);
    if path.is_file() {
        return path
            .file_stem()
            .and_then(|name| name.to_str())
            .unwrap_or(ident)
            .to_string();
    }
    if let Some(key) = normalize_builtin_key(ident) {
        return key;
    }
    Path::new(ident)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(ident)
        .to_string()
}

fn builtin_xml(ident: &str) -> Option<(&'static str, &'static str)> {
    let key = normalize_builtin_key(ident)?;
    match key.as_str() {
        "codesize" => Some((include_str!("../rulesets/codesize.xml"), "codesize")),
        "naming" => Some((include_str!("../rulesets/naming.xml"), "naming")),
        "unusedcode" => Some((include_str!("../rulesets/unusedcode.xml"), "unusedcode")),
        "cleancode" => Some((include_str!("../rulesets/cleancode.xml"), "cleancode")),
        "design" => Some((include_str!("../rulesets/design.xml"), "design")),
        "controversial" => Some((
            include_str!("../rulesets/controversial.xml"),
            "controversial",
        )),
        "rust" => Some((include_str!("../rulesets/rust.xml"), "rust")),
        "opinionated" => Some((include_str!("../rulesets/opinionated.xml"), "opinionated")),
        _ => None,
    }
}

fn normalize_builtin_key(ident: &str) -> Option<String> {
    let lower = ident.to_ascii_lowercase().replace('\\', "/");
    let base = Path::new(&lower)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or(&lower);
    let stem = base.strip_suffix(".xml").unwrap_or(base);
    match stem {
        "codesize" | "naming" | "unusedcode" | "cleancode" | "design" | "controversial"
        | "rust" | "opinionated" => Some(stem.to_string()),
        _ => None,
    }
}

fn split_ref(ref_str: &str) -> (String, String) {
    if is_resolvable(ref_str) {
        return (ref_str.to_string(), String::new());
    }
    if let Some(idx) = ref_str.rfind('/') {
        let base = &ref_str[..idx];
        if is_resolvable(base) {
            return (base.to_string(), ref_str[idx + 1..].to_string());
        }
    }
    (ref_str.to_string(), String::new())
}

fn is_resolvable(ident: &str) -> bool {
    builtin_xml(ident).is_some() || Path::new(ident).is_file()
}

fn parse_ruleset(xml: &str) -> Result<XmlRuleset, String> {
    let doc = roxmltree::Document::parse(xml).map_err(|e| format!("invalid ruleset XML: {e}"))?;
    let root = doc.root_element();
    if root.tag_name().name() != "ruleset" {
        return Err("ruleset XML root must be <ruleset>".to_string());
    }
    let name = root.attribute("name").unwrap_or("").to_string();
    let mut rules = Vec::new();
    for child in root.children().filter(|n| n.is_element()) {
        if child.tag_name().name() == "rule" {
            rules.push(parse_rule(child));
        }
    }
    Ok(XmlRuleset { name, rules })
}

fn parse_rule(node: roxmltree::Node<'_, '_>) -> XmlRule {
    let mut rule = XmlRule {
        name: node.attribute("name").unwrap_or("").to_string(),
        class: node.attribute("class").unwrap_or("").to_string(),
        message: node.attribute("message").unwrap_or("").to_string(),
        ref_path: node.attribute("ref").unwrap_or("").to_string(),
        priority: None,
        properties: BTreeMap::new(),
        excludes: Vec::new(),
    };
    for child in node.children().filter(|n| n.is_element()) {
        parse_rule_child(&mut rule, child);
    }
    rule
}

fn parse_rule_child(rule: &mut XmlRule, child: roxmltree::Node<'_, '_>) {
    match child.tag_name().name() {
        "priority" => {
            if let Ok(priority) = child.text().unwrap_or("").trim().parse::<u8>() {
                rule.priority = Some(priority);
            }
        }
        "properties" => parse_properties(rule, child),
        "exclude" => {
            if let Some(name) = child.attribute("name") {
                rule.excludes.push(name.to_string());
            }
        }
        _ => {}
    }
}

fn parse_properties(rule: &mut XmlRule, properties: roxmltree::Node<'_, '_>) {
    for property in properties.children().filter(|node| node.is_element()) {
        if property.tag_name().name() != "property" {
            continue;
        }
        let name = property.attribute("name").unwrap_or("");
        if !name.is_empty() {
            rule.properties
                .insert(name.to_string(), property_value(property));
        }
    }
}

fn property_value(property: roxmltree::Node<'_, '_>) -> String {
    let attribute = property.attribute("value").unwrap_or("");
    if !attribute.is_empty() {
        return attribute.to_string();
    }
    property
        .children()
        .find(|node| node.is_element() && node.tag_name().name() == "value")
        .and_then(|node| node.text())
        .unwrap_or("")
        .trim()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_context_diamond(
        dir: &Path,
        depth: usize,
        leaf: &str,
        left_override: &str,
        right_override: &str,
    ) -> PathBuf {
        for index in 0..depth {
            let current = dir.join(format!("context-{index}.xml"));
            let next = if index + 1 == depth {
                leaf.to_string()
            } else {
                dir.join(format!("context-{}.xml", index + 1))
                    .display()
                    .to_string()
            };
            fs::write(
                current,
                format!(
                    "<ruleset name=\"Context {index}\">\
                     <rule ref=\"{next}\">{left_override}</rule>\
                     <rule ref=\"{next}\">{right_override}</rule>\
                     </ruleset>"
                ),
            )
            .expect("write context diamond");
        }
        dir.join("context-0.xml")
    }

    fn load_with_counts(spec: &Path, opts: &LoadOptions) -> (usize, usize, Vec<LoadedRule>) {
        BLOCKER_STATUS_CALLS.with(|calls| calls.set(0));
        INTERSECTION_VISITS.with(|visits| visits.set(0));
        let mut warn = |_| {};
        let rules = load_and_filter(&[spec.display().to_string()], &[], &[], opts, &mut warn)
            .expect("load context diamond");
        let status_calls = BLOCKER_STATUS_CALLS.with(Cell::get);
        let intersection_visits = INTERSECTION_VISITS.with(Cell::get);
        (status_calls, intersection_visits, rules)
    }

    #[test]
    fn branch_specific_irrelevant_exclusions_have_a_linear_status_bound() {
        let dir = TempDir::new().expect("temporary directory");
        let leaf = dir.path().join("blocked.xml");
        fs::write(
            &leaf,
            "<ruleset name=\"Blocked\"><rule name=\"Blocked\" class=\"PHPMD\\Rule\\Design\\LongParameterList\"/></ruleset>",
        )
        .expect("write blocked leaf");
        let depth = 18;
        let first = write_context_diamond(
            dir.path(),
            depth,
            leaf.to_str().expect("leaf path"),
            "<exclude name=\"Blocked\"/><exclude name=\"DummyLeft\"/>",
            "<exclude name=\"Blocked\"/><exclude name=\"DummyRight\"/>",
        );

        let (status_calls, _, rules) = load_with_counts(&first, &LoadOptions::default());

        assert!(rules.is_empty());
        assert!(status_calls < depth * 20, "status calls: {status_calls}");
    }

    #[test]
    fn equivalent_filtered_priorities_have_a_linear_status_bound() {
        let dir = TempDir::new().expect("temporary directory");
        let depth = 18;
        let first = write_context_diamond(
            dir.path(),
            depth,
            "codesize/ExcessiveParameterList",
            "<priority>4</priority>",
            "<priority>5</priority>",
        );
        let opts = LoadOptions {
            min_priority: 3,
            max_priority: 1,
        };

        let (status_calls, _, rules) = load_with_counts(&first, &opts);

        assert!(rules.is_empty());
        assert!(status_calls < depth * 20, "status calls: {status_calls}");
    }

    #[test]
    fn unrelated_loaded_rules_do_not_invalidate_blocker_statuses() {
        let dir = TempDir::new().expect("temporary directory");
        let rule_count = 200;
        let reference_count = 40;
        let child = dir.path().join("blocked.xml");
        let mut child_xml = String::from("<ruleset name=\"Blocked\">");
        for index in 0..rule_count {
            child_xml.push_str(&format!(
                "<rule name=\"Blocked{index}\" class=\"PHPMD\\Rule\\Design\\LongParameterList\"/>"
            ));
        }
        child_xml.push_str("</ruleset>");
        fs::write(&child, child_xml).expect("write blocked ruleset");

        let root = dir.path().join("root.xml");
        let mut root_xml = String::from("<ruleset name=\"Root\">");
        for index in 0..reference_count {
            root_xml.push_str(&format!("<rule ref=\"{}\">", child.display()));
            for blocked_index in 0..rule_count {
                root_xml.push_str(&format!("<exclude name=\"Blocked{blocked_index}\"/>"));
            }
            root_xml.push_str("</rule>");
            root_xml.push_str(&format!(
                "<rule name=\"Unrelated{index}\" class=\"PHPMD\\Rule\\Design\\LongParameterList\"/>"
            ));
        }
        root_xml.push_str("</ruleset>");
        fs::write(&root, root_xml).expect("write root ruleset");

        RULE_STATUS_CALLS.with(|calls| calls.set(0));
        let mut warn = |_| {};
        let rules = load_and_filter(
            &[root.display().to_string()],
            &[],
            &[],
            &LoadOptions::default(),
            &mut warn,
        )
        .expect("load root ruleset");
        let rule_status_calls = RULE_STATUS_CALLS.with(Cell::get);

        assert_eq!(rules.len(), reference_count);
        assert!(
            rule_status_calls < rule_count,
            "rule status calls: {rule_status_calls}"
        );
    }

    #[test]
    fn deep_chain_reuses_intersections_for_the_shared_blocker_trie() {
        let dir = TempDir::new().expect("temporary directory");
        let rule_count = 200;
        let mut leaf_xml = String::from("<ruleset name=\"Leaf\">");
        for index in 0..rule_count {
            leaf_xml.push_str(&format!(
                "<rule name=\"Blocked{index}\" class=\"PHPMD\\Rule\\Design\\LongParameterList\"/>"
            ));
        }
        leaf_xml.push_str("</ruleset>");
        let leaf = dir.path().join("leaf.xml");
        fs::write(&leaf, leaf_xml).expect("write leaf ruleset");

        let depth = 40;
        let mut next = leaf;
        for index in (0..depth).rev() {
            let current = dir.path().join(format!("chain-{index}.xml"));
            fs::write(
                &current,
                format!(
                    "<ruleset name=\"Chain {index}\">\
                     <rule name=\"Local{index}\" class=\"PHPMD\\Rule\\Design\\LongParameterList\"/>\
                     <rule ref=\"{}\"/>\
                     </ruleset>",
                    next.display()
                ),
            )
            .expect("write chain ruleset");
            next = current;
        }

        let root = dir.path().join("root.xml");
        let mut root_xml = format!("<ruleset name=\"Root\"><rule ref=\"{}\">", next.display());
        for index in 0..rule_count {
            root_xml.push_str(&format!("<exclude name=\"Blocked{index}\"/>"));
        }
        for index in 0..depth {
            root_xml.push_str(&format!("<exclude name=\"Local{index}\"/>"));
        }
        root_xml.push_str("</rule></ruleset>");
        fs::write(&root, root_xml).expect("write root ruleset");

        let (_, intersection_visits, rules) = load_with_counts(&root, &LoadOptions::default());

        assert!(rules.is_empty());
        assert!(
            intersection_visits < 5_000,
            "intersection visits: {intersection_visits}"
        );
    }

    #[test]
    fn long_rule_names_use_iterative_trie_operations() {
        let prefix = "x".repeat(8_192);
        let left_name = format!("{prefix}Left");
        let right_name = format!("{prefix}Right");
        let left = name_trie_insert(&Rc::new(NameTrie::default()), &left_name);
        let right = name_trie_insert(&Rc::new(NameTrie::default()), &right_name);
        let blockers = name_trie_union(&left, &right);
        let mut interner = ContextNameInterner::default();
        let active = interner.insert(&ContextNameInterner::empty(), &right_name);

        let projected = interner.intersection(&blockers, &active);

        assert!(name_trie_contains(&blockers, &left_name));
        assert!(name_trie_contains(&blockers, &right_name));
        assert_ne!(projected.id, 0);
    }
}
