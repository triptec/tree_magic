//! This is documentation for the tree_magic crate.
//!
//! See the README in the repository for more information.

#[macro_use] extern crate nom;
#[macro_use] extern crate lazy_static;
extern crate petgraph;
extern crate fnv;

use std::collections::HashSet;
use petgraph::prelude::*;
use fnv::FnvHashMap;
//use petgraph::dot::{Dot, Config};

mod fdo_magic;
mod basetype;

#[cfg(feature="staticmime")] type MIME = &'static str;
#[cfg(not(feature="staticmime"))] type MIME = String;

/// Check these types first
const TYPEORDER: [&'static str; 6] =
[
	"image/png",
	"image/jpeg",
	"image/gif",
	"application/zip",
	"application/x-msdos-executable",
	"application/pdf"
];

struct CheckerStruct {
    from_u8: fn(&[u8], &str) -> bool,
    from_filepath: fn(&str, &str) -> bool,
    get_supported: fn() -> Vec<MIME>
}

/// List of checker functions
lazy_static! {
    static ref CHECKERS: Vec<CheckerStruct> = {vec![
        CheckerStruct{
            from_u8: fdo_magic::check::from_u8,
            from_filepath: fdo_magic::check::from_filepath,
            get_supported: fdo_magic::init::get_supported
        }, 
        CheckerStruct{
            from_u8: basetype::check::from_u8,
            from_filepath: basetype::check::from_filepath,
            get_supported: basetype::init::get_supported
        }
    ]};
}

/// Mappings between modules and supported mimes (by index in table above)
lazy_static! {
    static ref CHECKER_SUPPORT: FnvHashMap<MIME, usize> = {
        let mut out = FnvHashMap::<MIME, usize>::default();
        for i in 0..CHECKERS.len() {
            let supported_types = (CHECKERS[i].get_supported)();
            for j in supported_types {
                out.insert(j, i);
            }
        }
        out
    };
}

/// Information about currently loaded MIME types
///
/// The `graph` contains subclass relations between all given mimes.
/// (EX: `application/json` -> `text/plain` -> `application/octet-stream`)
/// This is a `petgraph` DiGraph, so you can walk the tree if needed.
/// 
/// The `hash` is a mapping between mime types and nodes on the graph.
/// The root of the graph is "all/all", so start traversing there unless
/// you need to jump to a particular node.
pub struct TypeStruct {
    pub graph: DiGraph<MIME, u32>,
    pub hash: FnvHashMap<MIME, NodeIndex>
}

lazy_static! {
    /// The TypeStruct autogenerated at library init, and used by the library.
    pub static ref TYPE: TypeStruct = {
        graph_init().unwrap_or( 
            TypeStruct{
                graph: DiGraph::new(),
                hash: FnvHashMap::default()
            } )
    };
}

#[cfg(not(feature="staticmime"))]
macro_rules! convmime {
    ($x:expr) => {$x.to_string()}
}
#[cfg(feature="staticmime")]
macro_rules! convmime {
    ($x:expr) => {$x}
}

#[cfg(not(feature="staticmime"))]
macro_rules! clonemime {
    ($x:expr) => {$x.clone()}
}
#[cfg(feature="staticmime")]
macro_rules! clonemime {
    ($x:expr) => {$x}
}

// Initialize filetype graph
fn graph_init() -> Result<TypeStruct, std::io::Error> {
    
    let mut graph = DiGraph::<MIME, u32>::new();
    let mut added_mimes = FnvHashMap::<MIME, NodeIndex>::default();
    
    // Get list of MIME types
    let mut mimelist = fdo_magic::init::get_supported();
    mimelist.extend(basetype::init::get_supported());
    
    mimelist.sort();
    mimelist.dedup();
    let mimelist = mimelist;
    
    // Create all nodes
    for mimetype in mimelist.iter() {
        let node = graph.add_node(clonemime!(mimetype));
        added_mimes.insert(clonemime!(mimetype), node);
    }
    
    // Get list of edges from each mod's init submod
    // TODO: Can we iterate over a vector of function/module pointers?
    let mut edge_list_raw = basetype::init::get_subclasses();
    edge_list_raw.extend(fdo_magic::init::get_subclasses());
        
    let mut edge_list = HashSet::<(NodeIndex, NodeIndex)>::with_capacity(edge_list_raw.len());
    for x in edge_list_raw {
        let child_raw = x.0;
        let parent_raw = x.1;
        
        let parent = match added_mimes.get(&parent_raw) {
            Some(node) => *node,
            None => {continue;}
        };
        
        let child = match added_mimes.get(&child_raw) {
            Some(node) => *node,
            None => {continue;}
        };
        
        edge_list.insert( (child, parent) );
    }
    
    graph.extend_with_edges(&edge_list);
    
    //Add to applicaton/octet-stream, all/all, or text/plain, depending on top-level
    //(We'll just do it here because having the graph makes it really nice)
    let added_mimes_tmp = added_mimes.clone();
    let node_text = match added_mimes_tmp.get("text/plain"){
        Some(x) => *x,
        None => {
            let node = graph.add_node(convmime!("text/plain"));
            added_mimes.insert(convmime!("text/plain"), node);
            node
        }
    };
    let node_octet = match added_mimes_tmp.get("application/octet-stream"){
        Some(x) => *x,
        None => {
            let node = graph.add_node(convmime!("application/octet-stream"));
            added_mimes.insert(convmime!("application/octet-stream"), node);
            node
        }
    };
    let node_allall = match added_mimes_tmp.get("all/all"){
        Some(x) => *x,
        None => {
            let node = graph.add_node(convmime!("all/all"));
            added_mimes.insert(convmime!("all/all"), node);
            node
        }
    };
    let node_allfiles = match added_mimes_tmp.get("all/allfiles"){
        Some(x) => *x,
        None => {
            let node = graph.add_node(convmime!("all/allfiles"));
            added_mimes.insert(convmime!("all/allfiles"), node);
            node
        }
    };
    
    let mut edge_list_2 = HashSet::<(NodeIndex, NodeIndex)>::new();
    for mimenode in graph.externals(Incoming) {
        
        let ref mimetype = graph[mimenode];
        let toplevel = mimetype.split("/").nth(0).unwrap_or("");
        
        if mimenode == node_text || mimenode == node_octet || 
           mimenode == node_allfiles || mimenode == node_allall 
        {
            continue;
        }
        
        if toplevel == "text" {
            edge_list_2.insert( (node_text, mimenode) );
        } else if toplevel == "inode" {
            edge_list_2.insert( (node_allall, mimenode) );
        } else {
            edge_list_2.insert( (node_octet, mimenode) );
        }
    }
    // Don't add duplicate entries
    graph.extend_with_edges(edge_list_2.difference(&edge_list));
    
    let graph = graph;
    let added_mimes = added_mimes;
    //println!("{:?}", Dot::with_config(&graph, &[Config::EdgeNoLabel]));

    Ok( TypeStruct{graph: graph, hash: added_mimes} )
}

/// Just the part of from_*_node that walks the graph
fn typegraph_walker<T: Clone>(
    parentnode: NodeIndex,
    input: T,
    matchfn: fn(&str, T) -> bool
) -> Option<MIME> {

    let mut children: Vec<NodeIndex> = TYPE.graph
        .neighbors_directed(parentnode, Outgoing)
        .collect();
        
    for i in 0..children.len() {
        let x = children[i];
        if TYPEORDER.contains(&&*TYPE.graph[x]) {
            children.remove(i);
            children.insert(0, x);
        }
    }

    for childnode in children {
        let ref mimetype = TYPE.graph[childnode];
        let result = (matchfn)(mimetype, input.clone());
        match result {
            true => {
                match typegraph_walker(
                    childnode, input, matchfn
                ) {
                    Some(foundtype) => return Some(foundtype),
                    None => return Some(clonemime!(mimetype)),
                }
            }
            false => continue,
        }
    }
    
    None
}

/// Checks if the given bytestream matches the given MIME type.
///
/// Returns true or false if it matches or not. If the given mime type is not known,
/// the function will always return false.
pub fn match_u8(mimetype: &str, bytes: &[u8]) -> bool
{
    match CHECKER_SUPPORT.get(mimetype) {
        None => false,
        Some(x) => (CHECKERS[*x].from_u8)(bytes, &mimetype)
    }
}


/// Gets the type of a file from a raw bytestream, starting at a certain node
/// in the type graph.
///
/// Returns mime as string wrapped in Some if a type matches, or
/// None if no match is found.
/// Retreive the node from the `TYPE.hash` FnvHashMap, using the MIME as the key.
///
/// ## Panics
/// Will panic if the given node is not found in the graph.
/// As the graph is immutable, this should not happen if the node index comes from
/// TYPE.hash.
pub fn from_u8_node(parentnode: NodeIndex, bytes: &[u8]) -> Option<MIME>
{
	typegraph_walker(parentnode, bytes, match_u8)
}

/// Gets the type of a file from a byte stream.
///
/// Returns mime as string wrapped in Some if a type matches, or
/// None if no match is found. Because this starts from the type graph root,
/// it is a bug if this returns None.
pub fn from_u8(bytes: &[u8]) -> Option<MIME>
{
    let node = match TYPE.graph.externals(Incoming).next() {
        Some(foundnode) => foundnode,
        None => return None
    };
    from_u8_node(node, bytes)
}

/// Check if the given filepath matches the given MIME type.
///
/// Returns true or false if it matches or not, or an Error if the file could
/// not be read. If the given mime type is not known, it will always return false.
pub fn match_filepath(mimetype: &str, filepath: &str) -> bool 
{
    match CHECKER_SUPPORT.get(mimetype) {
        None => false,
        Some(x) => (CHECKERS[*x].from_filepath)(filepath, &mimetype)
    }
}


/// Gets the type of a file from a filepath, starting at a certain node
/// in the type graph.
///
/// Returns mime as string wrapped in Some if a type matches, or
/// None if the file is not found or cannot be opened.
/// Retreive the node from the `TYPE.hash` FnvHashMap, using the MIME as the key.
///
/// ## Panics
/// Will panic if the given node is not found in the graph.
/// As the graph is immutable, this should not happen if the node index comes from
/// TYPE.hash.
pub fn from_filepath_node(parentnode: NodeIndex, filepath: &str) -> Option<MIME> 
{
    typegraph_walker(parentnode, filepath, match_filepath)
}

/// Gets the type of a file from a filepath.
///
/// Does not look at file name or extension, just the contents.
/// Returns mime as string wrapped in Some if a type matches, or
/// None if the file is not found or cannot be opened.
pub fn from_filepath(filepath: &str) -> Option<MIME> {

    let node = match TYPE.graph.externals(Incoming).next() {
        Some(foundnode) => foundnode,
        None => return None
    };
    
    from_filepath_node(node, filepath)
}
