use std::{collections::HashMap, sync::Arc};

#[derive(Hash, Eq, PartialEq, Clone)]
pub struct Class {
    uri: String,
    name: String,
}

impl Class {
    fn from_raw(c: &lilv::plugin::Class) -> Class {
        Class {
            uri: c.uri().unwrap().turtle_token(),
            name: c.label().turtle_token(),
        }
    }
}

// Returns the class and all its parents.
pub fn class_with_parents(
    raw_class: &lilv::plugin::Class,
    class_to_parent: &HashMap<Class, Arc<Class>>,
) -> Vec<String> {
    let mut class = Class::from_raw(&raw_class);
    let mut ret = vec![class.name.clone()];
    while let Some(parent) = class_to_parent.get(&class) {
        class = parent.as_ref().clone();
        ret.push(class.name.clone());
    }
    ret
}

pub fn make_class_to_parent_map(world: &lilv::World) -> HashMap<Class, Arc<Class>> {
    let top_class = match world.plugin_class() {
        Some(c) => c,
        None => return HashMap::new(),
    };
    let mut ret = HashMap::new();
    populate_class_to_parent_children(&top_class, Arc::new(Class::from_raw(&top_class)), &mut ret);
    ret
}

fn populate_class_to_parent_children(
    top: &lilv::plugin::Class,
    top_c: Arc<Class>,
    m: &mut HashMap<Class, Arc<Class>>,
) {
    for child in top.children().unwrap().iter() {
        let child_class = Class::from_raw(&child);
        m.insert(child_class.clone(), top_c.clone());
        populate_class_to_parent_children(&child, Arc::new(child_class), m);
    }
}
