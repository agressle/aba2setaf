//A simple tree structure to represent our work space

use crate::{on_error, EXIT_CODE_OTHER_ERROR};

pub struct Tree<T> {
    nodes: Vec<Node<T>>,
    free_nodes: Vec<usize>
}

impl<T> Tree<T> {
    #[inline(always)]
    pub fn new(root: T) -> Self {
        let root = Node::new(root, 0, None);
        let nodes = vec![root];
        Self {
            nodes,
            free_nodes: Vec::new()
        }
    }

    #[inline(always)]
    fn get_next_node(&mut self, data: T, parent: usize) -> usize {
        if let Some(node) = self.free_nodes.pop() {
            let Some(node) = self.nodes.get_mut(node) else {
                on_error("Failed to get free node", EXIT_CODE_OTHER_ERROR);
            };
            node.reset(data, parent);
            return node.get_index();
        }
        let node = Node::new(data, self.nodes.len(), Some(parent));
        self.nodes.push(node);
        let Some(node) = self.nodes.last_mut() else {
            unreachable!()
        };
        node.get_index()
    }
    
    #[inline(always)]
    pub fn add_child(&mut self, node: usize, data: T) {
        let child_node = self.get_next_node(data, node);
        let Some(node) = self.nodes.get_mut(node) else {
            on_error("Failed to get parent node", EXIT_CODE_OTHER_ERROR);
        };
        node.add_child(child_node);
    }

    #[inline(always)]
    fn get_node(&self, node: usize) -> &Node<T> {
        let Some(node) = self.nodes.get(node) else {
            on_error("Failed to find node", EXIT_CODE_OTHER_ERROR);
        };
        node
    }

    #[inline(always)]
    fn get_node_mut(&mut self, node: usize) -> &mut Node<T> {
        let Some(node) = self.nodes.get_mut(node) else {
            on_error("Failed to find node", EXIT_CODE_OTHER_ERROR);
        };
        node
    }

    #[inline(always)]
    pub fn get_data_and_parent(&self, node: usize) -> (&T, Option<usize>) {
        let node = self.get_node(node);
        let parent = node.get_parent();
        (node.get_data(), parent)
    }

    #[inline(always)]
    pub fn get(&self, node: usize) -> &T {
        self.get_node(node).get_data()
    }

    #[inline(always)]
    pub fn get_mut(&mut self, node: usize) -> &mut T {
        self.get_node_mut(node).get_data_mut()
    }

    #[inline(always)]
    pub fn get_parent(&self, node: usize) -> Option<usize> {
        let node = self.get_node(node);
        node.parent
    }

    #[inline(always)]
    pub fn get_children(&self, node: usize) -> &Vec<usize> {
        let node = self.get_node(node);
        &node.get_children()
    }

    #[inline(always)]
    pub fn remove_children_subtree(&mut self, node: usize) -> ()
    {
        let node = self.get_node_mut(node);
        let mut to_delete = node.drain_children();
        while let Some(node) = to_delete.pop() {
            for child in self.get_children(node) {
                to_delete.push(*child);
            }
            self.free_nodes.push(node);
        }
    }

    #[inline(always)]
    pub fn undo_child<F>(&mut self, node: usize, child_index: usize, mut undoing_function: F) -> ()
        where F: FnMut(&mut T) -> T
    {
        let node = self.get_node_mut(node);
        let child_index = node.get_children()[child_index];
        let child_data = self.get_mut(child_index);
        *child_data = undoing_function(child_data);
    }

}

struct Node<T> {
    data: T,
    index: usize,
    parent: Option<usize>,
    children: Vec<usize>
}

impl<T> Node<T> {

    #[inline(always)]
    pub fn new(data: T, index: usize, parent: Option<usize>) -> Self {
        Self {
            data,
            index,
            parent,
            children: Vec::new()
        }
    }

    #[inline(always)]
    pub fn get_index(&self) -> usize {
        self.index
    }
    
    #[inline(always)]
    pub fn get_data(&self) -> &T {
        &self.data
    }

    #[inline(always)]
    pub fn get_data_mut(&mut self) -> &mut T {
        &mut self.data
    }

    #[inline(always)]
    pub fn reset(&mut self, data: T, parent: usize) {
        self.data = data;
        self.parent = Some(parent);
        self.children.clear();
    }

    #[inline(always)]
    pub fn get_parent(&self) -> Option<usize> {
        self.parent
    }

    #[inline(always)]
    pub fn get_children(&self) -> &Vec<usize> {
        &self.children
    }

    #[inline(always)]
    pub fn drain_children(&mut self) -> Vec<usize> {
        self.children.drain(..).collect()
    }

    #[inline(always)]
    pub fn add_child(&mut self, child: usize) {
        self.children.push(child);
    }
}