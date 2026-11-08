//! Text formats of the upstream graph tests.
//!
//! Ported from `platform/vcs-log/graph/test/com/intellij/vcs/log/graph/parser/`
//! (`CommitParser.java`, `SimpleCommit.java`, `EdgeNodeCharConverter.java`, `LinearGraphParser.java`).

use std::collections::HashMap;

use ig_log::api::EdgeFilter;
use ig_log::api::elements::{GraphEdge, GraphEdgeType, GraphNode, GraphNodeType};
use ig_log::api::linear_graph::LinearGraph;
use ig_log::graph_commit::GraphCommit;

pub const SEPARATOR: &str = "|-";

/// A commit read from a test file (`SimpleCommit`).
#[derive(Clone, Debug)]
pub struct SimpleCommit<Id> {
    pub id: Id,
    pub parents: Vec<Id>,
    pub timestamp: i64,
}

impl<Id> GraphCommit<Id> for SimpleCommit<Id> {
    fn id(&self) -> &Id {
        &self.id
    }
    fn parents(&self) -> &[Id] {
        &self.parents
    }
    fn timestamp(&self) -> i64 {
        self.timestamp
    }
}

impl SimpleCommit<i32> {
    pub fn as_integer_commit(commit_hash: &str, parents_hashes: &[&str]) -> SimpleCommit<i32> {
        let int_commit_hash = create_hash(commit_hash);
        let parents = parents_hashes
            .iter()
            .filter(|hash| !hash.is_empty())
            .map(|hash| create_hash(hash))
            .collect();
        SimpleCommit {
            id: int_commit_hash,
            parents,
            timestamp: i64::from(int_commit_hash),
        }
    }
}

impl SimpleCommit<String> {
    pub fn as_string_commit(commit_hash: &str, parents_hashes: &[&str]) -> SimpleCommit<String> {
        let timestamp = create_hash(commit_hash);
        let parents = parents_hashes
            .iter()
            .filter(|hash| !hash.is_empty())
            .map(|hash| hash.to_string())
            .collect();
        SimpleCommit {
            id: commit_hash.to_string(),
            parents,
            timestamp: i64::from(timestamp),
        }
    }
}

pub fn next_separator_index(line: &str, start_index: usize) -> usize {
    match line[start_index..].find(SEPARATOR) {
        Some(index) => start_index + index,
        None => panic!(
            "not found separator \"{SEPARATOR}\", with startIndex={start_index}, in line: {line}"
        ),
    }
}

/// `ab123|-adada 193 352` -> (`ab123`, [`adada`, `193`, `352`]); `123|-` has no parents.
pub fn parse_commit_parents(line: &str) -> (&str, Vec<&str>) {
    let separator_index = next_separator_index(line, 0);
    let commit_hash_str = &line[..separator_index];
    let parent_hash_str = &line[separator_index + 2..];
    (commit_hash_str, split_whitespace_java(parent_hash_str))
}

/// Java `String.split("\\s")`: empty strings between consecutive separators are kept, trailing
/// empty strings dropped.
pub fn split_whitespace_java(s: &str) -> Vec<&str> {
    let mut parts: Vec<&str> = s.split([' ', '\t', '\n', '\x0B', '\x0C', '\r']).collect();
    while parts.len() > 1 && parts.last() == Some(&"") {
        parts.pop();
    }
    parts
}

pub fn create_hash(s: &str) -> i32 {
    i32::from_str_radix(s, 16).unwrap_or_else(|e| panic!("bad hash {s:?}: {e}"))
}

/// Non-empty lines.
pub fn to_lines(input: &str) -> Vec<&str> {
    input.split('\n').filter(|line| !line.is_empty()).collect()
}

pub fn parse_string_commit_list(input: &str) -> Vec<SimpleCommit<String>> {
    to_lines(input)
        .into_iter()
        .map(|line| {
            let (hash, parents) = parse_commit_parents(line);
            SimpleCommit::as_string_commit(hash, &parents)
        })
        .collect()
}

pub fn parse_integer_commit_list(input: &str) -> Vec<SimpleCommit<i32>> {
    to_lines(input)
        .into_iter()
        .map(|line| {
            let (hash, parents) = parse_commit_parents(line);
            SimpleCommit::as_integer_commit(hash, &parents)
        })
        .collect()
}

// EdgeNodeCharConverter

pub fn parse_graph_node_type(c: char) -> GraphNodeType {
    match c {
        'U' => GraphNodeType::Usual,
        'G' => GraphNodeType::Unmatched,
        'N' => GraphNodeType::NotLoadCommit,
        _ => panic!("Illegal char for graph node type: {c}"),
    }
}

pub fn parse_graph_edge_type(c: char) -> GraphEdgeType {
    match c {
        'U' => GraphEdgeType::Usual,
        'D' => GraphEdgeType::Dotted,
        'N' => GraphEdgeType::NotLoadCommit,
        'P' => GraphEdgeType::DottedArrowUp,
        'O' => GraphEdgeType::DottedArrowDown,
        _ => panic!("Illegal char for graph edge type: {c}"),
    }
}

pub fn edge_type_to_char(edge_type: GraphEdgeType) -> char {
    match edge_type {
        GraphEdgeType::Usual => 'U',
        GraphEdgeType::Dotted => 'D',
        GraphEdgeType::NotLoadCommit => 'N',
        GraphEdgeType::DottedArrowUp => 'P',
        GraphEdgeType::DottedArrowDown => 'O',
    }
}

pub fn node_type_to_char(node_type: GraphNodeType) -> char {
    match node_type {
        GraphNodeType::Usual => 'U',
        GraphNodeType::Unmatched => 'G',
        GraphNodeType::NotLoadCommit => 'N',
    }
}

// LinearGraphParser

/// Java `Integer.decode`: optional sign, then `0x`/`0X`/`#` hex, leading-zero octal, or decimal.
fn java_integer_decode(s: &str) -> i32 {
    let (negative, digits) = match s.strip_prefix('-') {
        Some(rest) => (true, rest),
        None => (false, s.strip_prefix('+').unwrap_or(s)),
    };
    let (radix, digits) = if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
        .or_else(|| digits.strip_prefix('#'))
    {
        (16, hex)
    } else if digits.len() > 1 && digits.starts_with('0') {
        (8, &digits[1..])
    } else {
        (10, digits)
    };
    let value =
        i64::from_str_radix(digits, radix).unwrap_or_else(|e| panic!("bad number {s:?}: {e}"));
    (if negative { -value } else { value }) as i32
}

fn parse_number_with_char(input: &str) -> (i32, char) {
    let node_id = java_integer_decode(&input[..input.len() - 2]);
    (node_id, input.chars().last().expect("empty element"))
}

struct ParsedLine {
    node_id: i32,
    graph_node: GraphNode,
    normal_edges: Vec<String>,
}

/// Example input line: `0_U|-1_U 2_D`.
fn parse_line(line: &str, line_number: i32) -> ParsedLine {
    let separator_index = next_separator_index(line, 0);
    let (node_id, node_char) = parse_number_with_char(&line[..separator_index]);
    let graph_node = GraphNode::with_type(line_number, parse_graph_node_type(node_char));
    let normal_edges = split_whitespace_java(&line[separator_index + 2..])
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect();
    ParsedLine {
        node_id,
        graph_node,
        normal_edges,
    }
}

/// `LinearGraphParser.parse`.
pub fn parse_linear_graph(input: &str) -> TestLinearGraphWithElementsInfo {
    let mut graph_nodes = Vec::new();
    let mut edges: Vec<Vec<String>> = Vec::new();
    let mut node_id_to_node_index = HashMap::new();

    for line in to_lines(input) {
        let parsed = parse_line(line, graph_nodes.len() as i32);
        edges.push(parsed.normal_edges);
        node_id_to_node_index.insert(parsed.node_id, graph_nodes.len() as i32);
        graph_nodes.push(parsed.graph_node);
    }

    let mut up_edges: HashMap<i32, Vec<GraphEdge>> = HashMap::new();
    let mut down_edges: HashMap<i32, Vec<GraphEdge>> = HashMap::new();
    for (graph_node, node_edges) in graph_nodes.iter().zip(&edges) {
        for str_edge in node_edges {
            let (node_id, edge_char) = parse_number_with_char(str_edge);
            let edge_type = parse_graph_edge_type(edge_char);

            let edge = match edge_type {
                GraphEdgeType::Usual | GraphEdgeType::Dotted => {
                    let down_node_index = *node_id_to_node_index
                        .get(&node_id)
                        .unwrap_or_else(|| panic!("unknown node {node_id}"));
                    GraphEdge::create_normal_edge(graph_node.node_index, down_node_index, edge_type)
                }
                GraphEdgeType::NotLoadCommit
                | GraphEdgeType::DottedArrowDown
                | GraphEdgeType::DottedArrowUp => GraphEdge::create_edge_with_target_id(
                    graph_node.node_index,
                    Some(node_id),
                    edge_type,
                ),
            };
            if let Some(up) = edge.up_node_index {
                down_edges.entry(up).or_default().push(edge);
            }
            if let Some(down) = edge.down_node_index {
                up_edges.entry(down).or_default().push(edge);
            }
        }
    }

    TestLinearGraphWithElementsInfo {
        graph_nodes,
        up_edges,
        down_edges,
    }
}

pub struct TestLinearGraphWithElementsInfo {
    graph_nodes: Vec<GraphNode>,
    up_edges: HashMap<i32, Vec<GraphEdge>>,
    down_edges: HashMap<i32, Vec<GraphEdge>>,
}

impl LinearGraph for TestLinearGraphWithElementsInfo {
    fn nodes_count(&self) -> i32 {
        self.graph_nodes.len() as i32
    }

    fn get_adjacent_edges(&self, node_index: i32, filter: EdgeFilter) -> Vec<GraphEdge> {
        let mut result = Vec::new();

        for up_edge in self.up_edges.get(&node_index).into_iter().flatten() {
            if up_edge.edge_type.is_normal_edge() && filter.up_normal() {
                result.push(*up_edge);
            }
            if !up_edge.edge_type.is_normal_edge() && filter.special() {
                result.push(*up_edge);
            }
        }

        for down_edge in self.down_edges.get(&node_index).into_iter().flatten() {
            if down_edge.edge_type.is_normal_edge() && filter.down_normal() {
                result.push(*down_edge);
            }
            if !down_edge.edge_type.is_normal_edge() && filter.special() {
                result.push(*down_edge);
            }
        }

        result
    }

    fn get_graph_node(&self, node_index: i32) -> GraphNode {
        self.graph_nodes[node_index as usize]
    }

    fn get_node_id(&self, node_index: i32) -> i32 {
        // upstream quirk kept: `nodeIndex > 0` rejects node 0 (only checked with assertions on)
        debug_assert!(
            node_index > 0 && node_index < self.nodes_count(),
            "Bad nodeIndex: {node_index}"
        );
        node_index
    }

    fn get_node_index(&self, node_id: i32) -> Option<i32> {
        (node_id >= 0 && node_id < self.nodes_count()).then_some(node_id)
    }
}
