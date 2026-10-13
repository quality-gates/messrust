use std::collections::{BTreeMap, BTreeSet};

use super::{
    body_line, owned_site, plain_site, values::capture_key, Binding, Bindings, Callable, Callback,
    Direction, Edge, Owner, Site, Summary,
};

pub(super) fn unchanged(tasks: &[Callable<'_>], previous: &[Summary]) -> bool {
    tasks.len() == previous.len()
        && tasks.iter().zip(previous).all(|(task, before)| {
            task.summary.sites == before.sites
                && task.summary.return_binding == before.return_binding
                && task.summary.edges == before.edges
        })
}

pub(super) fn solve(tasks: &mut [Callable<'_>]) {
    for _ in 0..64 {
        let before: Vec<_> = tasks
            .iter()
            .map(|task| task.summary.sites.clone())
            .collect();
        for current in 0..tasks.len() {
            let mut sites = tasks[current].summary.sites.clone();
            for edge in &tasks[current].summary.edges {
                for site in &before[edge.target] {
                    sites.extend(substitute(site, edge, current, tasks, &before, 0));
                }
            }
            tasks[current].summary.sites = sites;
        }
        if tasks
            .iter()
            .zip(&before)
            .all(|(task, sites)| &task.summary.sites == sites)
        {
            return;
        }
    }
    for task in tasks {
        task.summary.sites.insert(plain_site(
            Direction::Unknown,
            body_line(task.body),
            "its effect flow did not converge".to_string(),
        ));
    }
}

pub(super) fn finish(tasks: Vec<Callable<'_>>) -> Vec<Summary> {
    let writes: BTreeSet<_> = tasks
        .iter()
        .flat_map(|task| &task.summary.sites)
        .filter(|site| site.direction == Direction::Output)
        .filter_map(|site| site.owner.clone())
        .collect();
    tasks
        .into_iter()
        .map(|task| {
            let mut summary = task.summary;
            summary.sites.retain(|site| {
                !(site.direction == Direction::Input
                    && matches!(&site.owner, Some(Owner::Private(_)))
                    && !site
                        .owner
                        .as_ref()
                        .is_some_and(|owner| writes.contains(owner)))
            });
            summary
        })
        .collect()
}

fn substitute(
    site: &Site,
    edge: &Edge,
    recipient: usize,
    tasks: &[Callable<'_>],
    before: &[BTreeSet<Site>],
    depth: usize,
) -> Vec<Site> {
    if depth >= 8 {
        return vec![plain_site(
            Direction::Unknown,
            edge.line,
            "its callback flow exceeds the analysis limit".to_string(),
        )];
    }
    if let Some(callback) = &site.callback {
        return substitute_callback(site, callback, edge, recipient, tasks, before, depth);
    }
    if let Some(owner) = &site.owner {
        let owners = owner_arguments(owner, &edge.arguments, site.reachable);
        return owners
            .into_iter()
            .filter(|owner| visible(owner, site.direction, recipient))
            .map(|owner| owned_site(site.direction, owner, edge.line, site.reachable))
            .collect();
    }
    vec![Site {
        line: edge.line,
        ..site.clone()
    }]
}

fn visible(owner: &Owner, direction: Direction, recipient: usize) -> bool {
    match owner {
        Owner::Private(allocation) if allocation.callable == recipient => false,
        Owner::Parameter(_) if direction == Direction::Input => false,
        _ => true,
    }
}

fn substitute_callback(
    site: &Site,
    callback: &Callback,
    edge: &Edge,
    recipient: usize,
    tasks: &[Callable<'_>],
    before: &[BTreeSet<Site>],
    depth: usize,
) -> Vec<Site> {
    let Some(binding) = edge.arguments.get(&callback.name) else {
        return vec![Site {
            line: edge.line,
            ..site.clone()
        }];
    };
    let args: Vec<_> = callback
        .arguments
        .iter()
        .map(|arg| substitute_binding(arg, &edge.arguments))
        .collect();
    let mut result = Vec::new();
    for (target, captures) in &binding.callables {
        let mut arguments = captures.clone();
        arguments.extend(tasks[*target].params.iter().cloned().zip(args.clone()));
        let invocation = Edge {
            target: *target,
            line: edge.line,
            arguments,
        };
        for effect in &before[*target] {
            result.extend(substitute(
                effect,
                &invocation,
                recipient,
                tasks,
                before,
                depth + 1,
            ));
        }
    }
    if let Some(name) = &binding.callback {
        result.push(Site {
            callback: Some(Callback {
                name: name.clone(),
                arguments: args,
            }),
            line: edge.line,
            ..site.clone()
        });
    } else if binding.callables.is_empty() || binding.unresolved {
        result.push(Site {
            line: edge.line,
            callback: None,
            ..site.clone()
        });
    }
    result.retain(|site| !matches!(&site.owner, Some(Owner::Private(allocation)) if allocation.callable == edge.target));
    result
}

fn owner_arguments(owner: &Owner, arguments: &Bindings, elements: bool) -> BTreeSet<Owner> {
    let name = match owner {
        Owner::Parameter(name) => Some(name.clone()),
        Owner::Receiver => Some("self".to_string()),
        Owner::Captured(capture) => Some(capture_key(capture.callable, &capture.name)),
        _ => None,
    };
    if let Some(binding) = name.as_ref().and_then(|name| arguments.get(name)) {
        let mut owners = binding.outer.clone();
        if elements {
            owners.extend(binding.elements.iter().cloned());
        }
        owners
    } else {
        BTreeSet::from([owner.clone()])
    }
}

pub(super) fn substitute_binding(binding: &Binding, arguments: &Bindings) -> Binding {
    substitute_value(binding, arguments, 0)
}

fn substitute_value(binding: &Binding, arguments: &Bindings, depth: usize) -> Binding {
    if depth >= 8 {
        return Binding {
            unresolved: true,
            ..Binding::default()
        };
    }
    let mut result = binding.clone();
    result.outer = binding
        .outer
        .iter()
        .flat_map(|owner| owner_arguments(owner, arguments, false))
        .collect();
    result.elements = binding
        .elements
        .iter()
        .flat_map(|owner| owner_arguments(owner, arguments, true))
        .collect();
    result.parts = binding
        .parts
        .iter()
        .map(|part| substitute_value(part, arguments, depth + 1))
        .collect();
    result.callables = substitute_invocations(&binding.callables, arguments, depth);
    result.deferred = substitute_invocations(&binding.deferred, arguments, depth);
    result.deferred_callbacks = binding
        .deferred_callbacks
        .iter()
        .map(|(name, args)| {
            let renamed = arguments
                .get(name)
                .and_then(|value| value.callback.clone())
                .unwrap_or_else(|| name.clone());
            (
                renamed,
                args.iter()
                    .map(|arg| substitute_value(arg, arguments, depth + 1))
                    .collect(),
            )
        })
        .collect();
    if let Some(actual) = binding
        .callback
        .as_ref()
        .and_then(|name| arguments.get(name))
    {
        result.callback = actual.callback.clone();
        super::storage::merge_invocations(&mut result.callables, actual.callables.clone());
        super::storage::merge_invocations(&mut result.deferred, actual.deferred.clone());
        result
            .deferred_callbacks
            .extend(actual.deferred_callbacks.clone());
        result.unresolved |= actual.unresolved;
        if matches!(result.ty, super::ValueType::Unknown) {
            result.ty = actual.ty.clone();
        }
    }
    result
}

fn substitute_invocations(
    invocations: &BTreeMap<usize, Bindings>,
    arguments: &Bindings,
    depth: usize,
) -> BTreeMap<usize, Bindings> {
    invocations
        .iter()
        .map(|(target, captures)| {
            (
                *target,
                captures
                    .iter()
                    .map(|(name, value)| {
                        (name.clone(), substitute_value(value, arguments, depth + 1))
                    })
                    .collect(),
            )
        })
        .collect()
}
