use super::super::tr_val::TraversalVal;
use crate::{
    helix_engine::{
        graph_core::traversal_iter::RwTraversalIterator,
        types::GraphError,
    },
    protocol::{
        filterable::Filterable,
        items::{
            v6_uuid,
            Node,
            SerializedNode
        },
        value::Value
    },
};
use heed3::PutFlags;

pub struct AddNIterator {
    inner: std::iter::Once<Result<TraversalVal, GraphError>>,
}

impl Iterator for AddNIterator {
    type Item = Result<TraversalVal, GraphError>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next()
    }
}

pub trait AddNAdapter<'a, 'b>: Iterator<Item = Result<TraversalVal, GraphError>> + Sized {
    fn add_n(
        self,
        label: &'a str,
        properties: Vec<(String, Value)>,
        secondary_indices: Option<&'a [String]>,
        id: Option<u128>,
    ) -> RwTraversalIterator<'a, 'b, std::iter::Once<Result<TraversalVal, GraphError>>>;
}

impl<'a, 'b, I: Iterator<Item = Result<TraversalVal, GraphError>>> AddNAdapter<'a, 'b>
    for RwTraversalIterator<'a, 'b, I>
{
    fn add_n(
        self,
        label: &'a str,
        properties: Vec<(String, Value)>,
        secondary_indices: Option<&'a [String]>,
        id: Option<u128>, // TODO: can't be an option has to generated because always needs to be in order
    ) -> RwTraversalIterator<'a, 'b, std::iter::Once<Result<TraversalVal, GraphError>>> {
        let node = Node {
            id: id.unwrap_or(v6_uuid()),
            label: label.to_string(),
            properties: properties.into_iter().collect(),
        };

        let secondary_indices = secondary_indices.unwrap_or(&[]).to_vec();
        let mut result: Result<TraversalVal, GraphError> = Ok(TraversalVal::Empty);

        match SerializedNode::encode_node(&node) {
            Ok(bytes) => {
                if let Err(e) = self.storage.nodes_db.put_with_flags(
                    self.txn,
                    PutFlags::APPEND,
                    &node.id,
                    &bytes,
                ) {
                    result = Err(GraphError::from(e));
                }
            }
            Err(e) => result = Err(GraphError::from(e)),
        }

        for index in &secondary_indices {
            match self.storage.secondary_indices.get(index.as_str()) {
                Some(db) => {
                    let key = match node.check_property(&index) {
                        Some(value) => value,
                        None => {
                            result = Err(GraphError::New(format!(
                                "Secondary Index {} not found",
                                index
                            )));
                            continue;
                        }
                    };
                    match bincode::serialize(&key) {
                        Ok(serialized) => {
                            if let Err(e) = db.put(self.txn, &serialized, &node.id.to_be_bytes()) {
                                result = Err(GraphError::from(e));
                            }
                        }
                        Err(e) => result = Err(GraphError::from(e)),
                    }
                }
                None => {
                    result = Err(GraphError::New(format!(
                        "Secondary Index {} not found",
                        index
                    )));
                }
            }
        }

        if result.is_ok() {
            result = Ok(TraversalVal::Node(node.clone()));
        } else {
            result = Err(GraphError::New(format!(
                "Failed to add node to secondary indices"
            )));
        }

        RwTraversalIterator {
            inner: std::iter::once(result),
            storage: self.storage,
            txn: self.txn,
        }
    }
}