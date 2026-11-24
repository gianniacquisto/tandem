//! The notes graph read (ADR 0054): typed nodes, `contains` and `links` edges,
//! the project scope and the node cap, over a staged vault and a `BoardNames`.

use crate::notes::wikilink::{BoardNames, BoardProject};
use crate::notes::{GraphScope, NoteGraph, NoteGraphNode};

use super::notes_support::Vault;

fn names() -> BoardNames {
    BoardNames {
        projects: vec![
            BoardProject {
                name: "Tandem".to_string(),
                labels: vec!["Tandem".to_string(), "cli".to_string()],
            },
            BoardProject {
                name: "code-rank".to_string(),
                labels: vec!["code-rank".to_string()],
            },
        ],
    }
}

fn graph(vault: &Vault) -> NoteGraph {
    vault
        .notes()
        .graph(&names(), &GraphScope::default())
        .expect("the graph reads")
}

fn scoped(vault: &Vault, project: &str) -> NoteGraph {
    vault
        .notes()
        .graph(
            &names(),
            &GraphScope {
                project: Some(project.to_string()),
            },
        )
        .expect("the scoped graph reads")
}

fn node<'a>(graph: &'a NoteGraph, id: &str) -> &'a NoteGraphNode {
    graph
        .nodes
        .iter()
        .find(|node| node.id == id)
        .unwrap_or_else(|| panic!("no node {id}"))
}

fn has_node(graph: &NoteGraph, id: &str) -> bool {
    graph.nodes.iter().any(|node| node.id == id)
}

fn edge(graph: &NoteGraph, source: &str, target: &str, kind: &str) -> usize {
    graph
        .edges
        .iter()
        .filter(|e| e.source == source && e.target == target && e.kind.word() == kind)
        .count()
}

fn staged() -> Vault {
    let vault = Vault::new("graph");
    vault.stage("projects/tandem/Overview.md", "# Overview");
    vault.stage("projects/tandem/cli/Verbs.md", "# Verbs");
    vault.stage("projects/tandem/App/Screen.md", "# Screen");
    vault.stage("projects/code-rank/Rank.md", "# Rank");
    vault.stage("daily/2026-11-20.md", "Worked on [[label:cli]].");
    vault.stage("daily/2026-11-21.md", "Nothing linked.");
    vault.stage("Archive/Old.md", "# Old");
    vault
}

#[test]
fn folders_are_typed_by_the_board_and_unmatched_ones_say_why() {
    let graph = graph(&staged());
    let project = node(&graph, "projects/tandem");
    assert_eq!(project.kind.word(), "project");
    assert_eq!(project.name, "Tandem");
    assert_eq!(node(&graph, "projects/tandem/cli").kind.word(), "label");
    let app = node(&graph, "projects/tandem/App");
    assert_eq!(app.kind.word(), "folder");
    assert_eq!(app.unmatched.as_deref(), Some("no label"));
    assert_eq!(node(&graph, "projects").kind.word(), "folder");
    assert_eq!(node(&graph, "projects").unmatched, None);
    assert_eq!(node(&graph, "Archive").kind.word(), "folder");
}

#[test]
fn a_label_named_like_its_project_is_the_project_node() {
    let vault = staged();
    vault.stage("projects/code-rank/code-rank/Inner.md", "# Inner");
    let graph = graph(&vault);
    let like_project: Vec<_> = graph
        .nodes
        .iter()
        .filter(|node| node.name.eq_ignore_ascii_case("code-rank") && node.kind.word() != "note")
        .collect();
    assert_eq!(
        like_project.len(),
        2,
        "the project node and the plain folder"
    );
    assert_eq!(node(&graph, "projects/code-rank").kind.word(), "project");
    assert_eq!(
        node(&graph, "projects/code-rank/code-rank").kind.word(),
        "folder"
    );
}

#[test]
fn contains_edges_run_from_each_folder_to_its_children() {
    let graph = graph(&staged());
    assert_eq!(edge(&graph, "projects", "projects/tandem", "contains"), 1);
    assert_eq!(
        edge(&graph, "projects/tandem", "projects/tandem/cli", "contains"),
        1
    );
    assert_eq!(
        edge(
            &graph,
            "projects/tandem/cli",
            "projects/tandem/cli/Verbs.md",
            "contains"
        ),
        1
    );
    assert_eq!(edge(&graph, "daily", "daily/2026-11-20.md", "contains"), 1);
    assert!(
        !graph.edges.iter().any(|e| e.target == "daily"),
        "no vault-root node, no parent edge for a top-level folder"
    );
}

#[test]
fn daily_notes_link_and_float_by_the_orphan_rule() {
    let graph = graph(&staged());
    let linking = node(&graph, "daily/2026-11-20.md");
    assert!(linking.daily);
    assert!(!linking.orphan);
    assert_eq!(linking.name, "2026-11-20");
    assert_eq!(
        edge(
            &graph,
            "daily/2026-11-20.md",
            "projects/tandem/cli",
            "links"
        ),
        1
    );
    let lonely = node(&graph, "daily/2026-11-21.md");
    assert!(lonely.daily && lonely.orphan);
    assert!(!node(&graph, "projects/tandem/Overview.md").orphan);
    assert!(!node(&graph, "Archive/Old.md").daily);
}

#[test]
fn repeated_links_are_one_edge_self_links_none_and_broken_links_add_nothing() {
    let vault = Vault::new("graph-dedupe");
    vault.stage("A.md", "[[B]] and [[B]] again, [[A]] itself, [[Nowhere]]");
    vault.stage("B.md", "# B");
    let graph = graph(&vault);
    assert_eq!(edge(&graph, "A.md", "B.md", "links"), 1);
    assert_eq!(edge(&graph, "A.md", "A.md", "links"), 0);
    assert_eq!(graph.nodes.len(), 2);
    assert!(graph.edges.iter().all(|e| e.kind.word() != "contains"));
    assert_eq!(graph.edges.len(), 1);
}

#[test]
fn a_typed_link_to_a_project_with_no_folder_gets_a_pathless_node() {
    let vault = Vault::new("graph-virtual");
    vault.stage("Note.md", "[[project:code-rank]] and [[label:cli]]");
    let graph = graph(&vault);
    let project = node(&graph, "project:code-rank");
    assert_eq!(project.kind.word(), "project");
    assert_eq!(project.path, None);
    assert_eq!(node(&graph, "label:cli").path, None);
    assert_eq!(edge(&graph, "Note.md", "project:code-rank", "links"), 1);
}

#[test]
fn a_project_scope_keeps_its_subtree_and_what_links_into_it() {
    let graph = scoped(&staged(), "tandem");
    for id in [
        "projects/tandem",
        "projects/tandem/cli",
        "projects/tandem/App",
        "projects/tandem/Overview.md",
        "projects/tandem/cli/Verbs.md",
        "daily/2026-11-20.md",
        "daily",
    ] {
        assert!(has_node(&graph, id), "{id} is in scope");
    }
    for id in [
        "projects",
        "projects/code-rank",
        "projects/code-rank/Rank.md",
        "Archive/Old.md",
        "daily/2026-11-21.md",
    ] {
        assert!(!has_node(&graph, id), "{id} is out of scope");
    }
    assert_eq!(edge(&graph, "daily", "daily/2026-11-20.md", "contains"), 1);
    assert!(
        graph
            .edges
            .iter()
            .all(|e| has_node(&graph, &e.source) && has_node(&graph, &e.target))
    );
    assert_eq!(graph.total, graph.nodes.len());
}

#[test]
fn an_unknown_project_is_an_error_naming_the_projects() {
    let error = staged()
        .notes()
        .graph(
            &names(),
            &GraphScope {
                project: Some("Nope".to_string()),
            },
        )
        .expect_err("an unknown project is refused");
    let message = error.message();
    assert!(message.contains("Nope"), "{message}");
    assert!(
        message.contains("Tandem") && message.contains("code-rank"),
        "{message}"
    );
}

#[test]
fn past_the_cap_every_folder_stays_and_notes_go_by_link_degree() {
    let vault = Vault::new("graph-cap");
    vault.stage("projects/tandem/Overview.md", "# Overview");
    vault.stage("Hub.md", "[[Spoke1]] [[Spoke2]]");
    vault.stage("Spoke1.md", "# one");
    vault.stage("Spoke2.md", "# two");
    for n in 0..5 {
        vault.stage(&format!("Plain{n}.md"), "# plain");
    }
    let uncapped = vault
        .notes()
        .graph(&names(), &GraphScope::default())
        .expect("reads");
    let folders = uncapped
        .nodes
        .iter()
        .filter(|node| node.kind.word() != "note")
        .count();
    let capped = vault
        .notes()
        .graph_capped(&names(), &GraphScope::default(), folders + 2)
        .expect("reads");
    assert_eq!(capped.total, uncapped.total);
    assert_eq!(capped.shown, folders + 2);
    assert_eq!(capped.nodes.len(), capped.shown);
    assert_eq!(capped.cap, folders + 2);
    assert!(has_node(&capped, "projects/tandem") && has_node(&capped, "projects"));
    assert!(has_node(&capped, "Hub.md") && has_node(&capped, "Spoke1.md"));
    assert!(!has_node(&capped, "Spoke2.md") && !has_node(&capped, "Plain0.md"));
    assert!(
        capped
            .edges
            .iter()
            .all(|e| has_node(&capped, &e.source) && has_node(&capped, &e.target))
    );
}

#[test]
fn the_answer_order_is_stable_across_reads() {
    let vault = staged();
    let first = graph(&vault);
    assert_eq!(first, graph(&vault));
    let ids: Vec<&str> = first.nodes.iter().map(|node| node.id.as_str()).collect();
    let mut sorted = ids.clone();
    sorted.sort_unstable();
    assert_eq!(ids, sorted);
}
