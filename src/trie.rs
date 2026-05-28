//A simple trie implementation used for subsumption checking

use std::collections::{hash_map::Entry, HashMap};

struct Node {
    is_contained: bool,
    children: HashMap<usize, usize>
}

pub struct Trie {
    nodes: Vec<Node>
}

impl Trie {

    #[inline(always)]
    pub fn new() -> Self {
        Trie {
            nodes: vec!{Node {
                is_contained: false,
                children: HashMap::new()
            }}
        }
    }

    #[inline(always)]
    pub fn insert(&mut self, values: &Vec<usize>) {
        //Start with the root node
        let mut next_index = self.nodes.len();
        let mut current_node: &mut Node = &mut self.nodes[0];

        //Follow or create the path to the leaf node for the given values
        for value in values {
            let entry = current_node.children.entry(*value);
            let node_index = match entry {
                Entry::Occupied(entry) => *entry.get(),
                Entry::Vacant(entry) => {
                    entry.insert(next_index);
                    self.nodes.push(Node {
                        is_contained: false,
                        children: HashMap::new()
                    });
                    let index = next_index;
                    next_index += 1;
                    index
                }
            };
            current_node = &mut self.nodes[node_index];
        }

        //Mark the leaf node as contained
        current_node.is_contained = true;

    }

    #[inline(always)]
    pub fn contains_subset_of(&mut self, values: &Vec<usize>) -> bool {
        
        let mut nodes_to_process = Vec::new();
        nodes_to_process.push((0, 0));

        while let Some((node_index, value_index)) = nodes_to_process.pop() {
            let node = &self.nodes[node_index];
            
            if node.is_contained {
                return true;
            }
            
            if value_index >= values.len() {
                continue;
            }

            //Add the current node with incremented index. Case where the current element is not contained but there might still be a subset without the current element
            nodes_to_process.push((node_index, value_index + 1));

            let value = values[value_index];
            if let Some(child_index) = node.children.get(&value) {
                nodes_to_process.push((*child_index, value_index + 1)); //The current element is contained. Thus we add it second so that we go depth first
            }
        }
        
        return false;
    }
}