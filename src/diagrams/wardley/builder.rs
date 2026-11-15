use std::collections::HashMap;

use crate::errors::{ErrorCode, PeleError, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceStrategy {
    Build,
    Buy,
    Outsource,
    Market,
}

impl SourceStrategy {
    pub fn from_str_name(s: &str) -> Option<Self> {
        match s {
            "build" => Some(Self::Build),
            "buy" => Some(Self::Buy),
            "outsource" => Some(Self::Outsource),
            "market" => Some(Self::Market),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Build => "build",
            Self::Buy => "buy",
            Self::Outsource => "outsource",
            Self::Market => "market",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Forward,
    Backward,
    Bidirectional,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dimensions {
    pub width: f64,
    pub height: f64,
}

// Coordinates are percentages: x is evolution, y is visibility.
#[derive(Debug, Clone)]
pub struct WardleyNode {
    pub id: String,
    pub label: String,
    pub x: Option<f64>,
    pub y: Option<f64>,
    // 'anchor', 'component', or 'pipeline-component'
    pub class_name: Option<String>,
    pub label_offset_x: Option<f64>,
    pub label_offset_y: Option<f64>,
    pub in_pipeline: Option<bool>,
    pub is_pipeline_parent: Option<bool>,
    pub inertia: Option<bool>,
    pub source_strategy: Option<SourceStrategy>,
}

// Distinguishes omitted properties vs explicit `undefined` in JS `{ ...existing, ...node }`.
#[derive(Debug, Clone)]
pub struct NodeInput {
    pub id: String,
    pub label: String,
    pub x: Option<f64>,
    pub y: Option<f64>,
    pub class_name: Option<String>,
    pub label_offset_x: Option<f64>,
    pub label_offset_y: Option<f64>,
    pub inertia: Option<Option<bool>>,
    pub source_strategy: Option<Option<SourceStrategy>>,
}

#[derive(Debug, Clone)]
pub struct WardleyLink {
    pub source: String,
    pub target: String,
    pub dashed: bool,
    pub label: Option<String>,
    pub flow: Option<Flow>,
}

#[derive(Debug, Clone)]
pub struct WardleyTrend {
    pub node_id: String,
    pub target_x: f64,
    pub target_y: f64,
}

#[derive(Debug, Clone)]
pub struct WardleyPipeline {
    pub node_id: String,
    pub component_ids: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct WardleyAnnotation {
    pub number: f64,
    pub coordinates: Vec<Point>,
    pub text: Option<String>,
}

#[derive(Debug, Clone)]
pub struct WardleyNote {
    pub text: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone)]
pub struct WardleyForce {
    pub name: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, Default)]
pub struct WardleyAxesConfig {
    pub x_label: Option<String>,
    pub y_label: Option<String>,
    pub stages: Option<Vec<String>>,
    // Right edge of each stage, from 0 to 1.
    pub stage_boundaries: Option<Vec<f64>>,
}

#[derive(Debug, Clone)]
pub struct WardleyBuildResult {
    pub nodes: Vec<WardleyNode>,
    pub links: Vec<WardleyLink>,
    pub trends: Vec<WardleyTrend>,
    pub pipelines: Vec<WardleyPipeline>,
    pub annotations: Vec<WardleyAnnotation>,
    pub notes: Vec<WardleyNote>,
    pub accelerators: Vec<WardleyForce>,
    pub deaccelerators: Vec<WardleyForce>,
    pub annotations_box: Option<Point>,
    pub axes: WardleyAxesConfig,
    pub size: Option<Dimensions>,
}

#[derive(Debug, Clone)]
struct OrderMap<V> {
    index: HashMap<String, usize>,
    entries: Vec<(String, V)>,
}

impl<V> Default for OrderMap<V> {
    fn default() -> Self {
        Self { index: HashMap::new(), entries: Vec::new() }
    }
}

impl<V> OrderMap<V> {
    fn contains_key(&self, key: &str) -> bool {
        self.index.contains_key(key)
    }

    fn get(&self, key: &str) -> Option<&V> {
        self.index.get(key).map(|&idx| &self.entries[idx].1)
    }

    fn get_mut(&mut self, key: &str) -> Option<&mut V> {
        self.index.get(key).copied().map(move |idx| &mut self.entries[idx].1)
    }

    fn insert(&mut self, key: String, value: V) {
        if let Some(&idx) = self.index.get(&key) {
            self.entries[idx].1 = value;
        } else {
            let idx = self.entries.len();
            self.index.insert(key.clone(), idx);
            self.entries.push((key, value));
        }
    }

    fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }

    fn into_values(self) -> Vec<V> {
        self.entries.into_iter().map(|(_, v)| v).collect()
    }
}

#[derive(Debug, Default)]
pub struct WardleyBuilder {
    nodes: OrderMap<WardleyNode>,
    labels: Option<HashMap<String, String>>,
    links: Vec<WardleyLink>,
    trends: OrderMap<WardleyTrend>,
    pipelines: OrderMap<WardleyPipeline>,
    annotations: Vec<WardleyAnnotation>,
    notes: Vec<WardleyNote>,
    accelerators: Vec<WardleyForce>,
    deaccelerators: Vec<WardleyForce>,
    annotations_box: Option<Point>,
    axes: WardleyAxesConfig,
    size: Option<Dimensions>,
}

impl WardleyBuilder {
    // A node added again keeps its class and label offsets unless the new one sets them.
    pub fn add_node(&mut self, node: NodeInput) {
        let existing = self.nodes.get(&node.id).cloned().unwrap_or(WardleyNode {
            id: node.id.clone(),
            label: node.label.clone(),
            x: None,
            y: None,
            class_name: None,
            label_offset_x: None,
            label_offset_y: None,
            in_pipeline: None,
            is_pipeline_parent: None,
            inertia: None,
            source_strategy: None,
        });
        let merged = WardleyNode {
            id: node.id.clone(),
            label: node.label,
            x: node.x.or(existing.x),
            y: node.y.or(existing.y),
            class_name: node.class_name.or(existing.class_name),
            label_offset_x: node.label_offset_x.or(existing.label_offset_x),
            label_offset_y: node.label_offset_y.or(existing.label_offset_y),
            in_pipeline: existing.in_pipeline,
            is_pipeline_parent: existing.is_pipeline_parent,
            inertia: node.inertia.unwrap_or(existing.inertia),
            source_strategy: node.source_strategy.unwrap_or(existing.source_strategy),
        };
        self.nodes.insert(node.id, merged);
        self.labels = None;
    }

    pub fn add_link(&mut self, link: WardleyLink) {
        self.links.push(link);
    }

    pub fn add_trend(&mut self, trend: WardleyTrend) {
        self.trends.insert(trend.node_id.clone(), trend);
    }

    pub fn start_pipeline(&mut self, node_id: &str) {
        self.pipelines.insert(
            node_id.to_string(),
            WardleyPipeline { node_id: node_id.to_string(), component_ids: Vec::new() },
        );
        if let Some(node) = self.nodes.get_mut(node_id) {
            node.is_pipeline_parent = Some(true);
        }
    }

    pub fn add_pipeline_component(&mut self, pipeline_node_id: &str, component_id: &str) {
        if let Some(pipeline) = self.pipelines.get_mut(pipeline_node_id) {
            pipeline.component_ids.push(component_id.to_string());
        }
        if let Some(node) = self.nodes.get_mut(component_id) {
            node.in_pipeline = Some(true);
        }
    }

    pub fn add_annotation(&mut self, annotation: WardleyAnnotation) {
        self.annotations.push(annotation);
    }

    pub fn add_note(&mut self, note: WardleyNote) {
        self.notes.push(note);
    }

    pub fn add_accelerator(&mut self, accelerator: WardleyForce) {
        self.accelerators.push(accelerator);
    }

    pub fn add_deaccelerator(&mut self, deaccelerator: WardleyForce) {
        self.deaccelerators.push(deaccelerator);
    }

    pub fn set_annotations_box(&mut self, x: f64, y: f64) {
        self.annotations_box = Some(Point { x, y });
    }

    pub fn set_axes(&mut self, stages: Vec<String>, stage_boundaries: Vec<f64>) {
        self.axes.stages = Some(stages);
        self.axes.stage_boundaries = Some(stage_boundaries);
    }

    pub fn set_size(&mut self, width: f64, height: f64) {
        self.size = Some(Dimensions { width, height });
    }

    pub fn get_node(&self, id: &str) -> Option<&WardleyNode> {
        self.nodes.get(id)
    }

    // A name is a node id, or else the label of the first node that has it. Pipeline components
    // have ids like "Parent_Child" and are linked by label.
    pub fn resolve_node_id(&mut self, name: &str) -> String {
        if self.nodes.contains_key(name) {
            return name.to_string();
        }
        let labels = self.labels.get_or_insert_with(|| {
            let mut map = HashMap::new();
            for (id, node) in self.nodes.iter() {
                if !map.contains_key(&node.label) {
                    map.insert(node.label.clone(), id.to_string());
                }
            }
            map
        });
        labels.get(name).cloned().unwrap_or_else(|| name.to_string())
    }

    pub fn build(self) -> Result<WardleyBuildResult> {
        let nodes = self.nodes.into_values();
        for node in &nodes {
            if node.x.is_none() || node.y.is_none() {
                return Err(PeleError::new(
                    format!("Node \"{}\" is missing coordinates", node.label),
                    ErrorCode::Semantic,
                )
                .of("wardley"));
            }
        }
        Ok(WardleyBuildResult {
            nodes,
            links: self.links,
            trends: self.trends.into_values(),
            pipelines: self.pipelines.into_values(),
            annotations: self.annotations,
            notes: self.notes,
            accelerators: self.accelerators,
            deaccelerators: self.deaccelerators,
            annotations_box: self.annotations_box,
            axes: self.axes,
            size: self.size,
        })
    }
}
