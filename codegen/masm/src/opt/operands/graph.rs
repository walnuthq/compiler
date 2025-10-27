//! A lightweight directed graph implementation for operand dependency tracking.
//!
//! This module provides a simple directed graph structure and Kosaraju's algorithm
//! for computing strongly connected components, replacing the petgraph dependency
//! for no-std compatibility.

use alloc::collections::{BTreeMap, BTreeSet};
use alloc::vec::Vec;

use super::Operand;

/// A lightweight directed graph for tracking operand dependencies.
///
/// Nodes are of type `Operand` and edges have no associated data.
/// The graph maintains both forward (outgoing) and reverse (incoming) adjacency lists
/// for efficient neighbor queries in both directions.
#[derive(Default)]
pub struct DiGraph {
    /// Forward adjacency list: node -> set of nodes it points to
    outgoing: BTreeMap<Operand, BTreeSet<Operand>>,
    /// Reverse adjacency list: node -> set of nodes pointing to it
    incoming: BTreeMap<Operand, BTreeSet<Operand>>,
}

impl DiGraph {
    /// Creates a new empty directed graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a node to the graph. Returns the node itself.
    ///
    /// If the node already exists, this is a no-op.
    pub fn add_node(&mut self, node: Operand) -> Operand {
        self.outgoing.entry(node).or_default();
        self.incoming.entry(node).or_default();
        node
    }

    /// Adds a directed edge from `from` to `to`.
    ///
    /// Both nodes are automatically added to the graph if they don't exist.
    /// Multiple edges between the same pair of nodes are not duplicated.
    pub fn add_edge(&mut self, from: Operand, to: Operand) {
        self.outgoing.entry(from).or_default().insert(to);
        self.incoming.entry(to).or_default().insert(from);

        // Ensure both nodes exist in both maps
        self.outgoing.entry(to).or_default();
        self.incoming.entry(from).or_default();
    }

    /// Returns an iterator over the neighbors of a node in the specified direction.
    pub fn neighbors(&self, node: Operand, direction: Direction) -> impl Iterator<Item = Operand> + '_ {
        let map = match direction {
            Direction::Outgoing => &self.outgoing,
            Direction::Incoming => &self.incoming,
        };

        map.get(&node)
            .into_iter()
            .flat_map(|set| set.iter().copied())
    }

    /// Returns all nodes in the graph.
    pub fn nodes(&self) -> impl Iterator<Item = Operand> + '_ {
        self.outgoing.keys().copied()
    }
}

/// Direction for traversing graph edges.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Direction {
    /// Outgoing edges (from a node to its successors)
    Outgoing,
    /// Incoming edges (from predecessors to a node)
    Incoming,
}

/// Computes the strongly connected components of a directed graph using Kosaraju's algorithm.
///
/// Returns a vector of components, where each component is a vector of nodes.
/// Components are returned in reverse topological order.
///
/// # Algorithm
///
/// Kosaraju's algorithm works in two passes:
/// 1. Perform a DFS on the original graph to compute finish times
/// 2. Perform a DFS on the transposed graph in decreasing order of finish times
///
/// Each DFS tree in the second pass corresponds to one strongly connected component.
pub fn kosaraju_scc(graph: &DiGraph) -> Vec<Vec<Operand>> {
    let mut visited = BTreeSet::new();
    let mut finish_order = Vec::new();

    // First DFS pass: compute finish order
    for node in graph.nodes() {
        if !visited.contains(&node) {
            dfs_finish_order(graph, node, &mut visited, &mut finish_order);
        }
    }

    // Second DFS pass: find SCCs in reverse finish order
    let mut visited = BTreeSet::new();
    let mut components = Vec::new();

    for &node in finish_order.iter().rev() {
        if !visited.contains(&node) {
            let mut component = Vec::new();
            dfs_component(graph, node, &mut visited, &mut component);
            components.push(component);
        }
    }

    components
}

/// DFS helper for computing finish order (first pass of Kosaraju's algorithm).
fn dfs_finish_order(
    graph: &DiGraph,
    node: Operand,
    visited: &mut BTreeSet<Operand>,
    finish_order: &mut Vec<Operand>,
) {
    visited.insert(node);

    for neighbor in graph.neighbors(node, Direction::Outgoing) {
        if !visited.contains(&neighbor) {
            dfs_finish_order(graph, neighbor, visited, finish_order);
        }
    }

    finish_order.push(node);
}

/// DFS helper for collecting nodes in a component (second pass of Kosaraju's algorithm).
///
/// This traverses the graph in reverse direction (incoming edges).
fn dfs_component(
    graph: &DiGraph,
    node: Operand,
    visited: &mut BTreeSet<Operand>,
    component: &mut Vec<Operand>,
) {
    visited.insert(node);
    component.push(node);

    for neighbor in graph.neighbors(node, Direction::Incoming) {
        if !visited.contains(&neighbor) {
            dfs_component(graph, neighbor, visited, component);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests are primarily for verifying the graph structure and SCC algorithm.
    // The actual operand values are tested in the integration tests that use real HIR values.

    #[test]
    fn test_empty_graph() {
        let graph = DiGraph::new();
        let components = kosaraju_scc(&graph);
        assert!(components.is_empty());
    }
}
