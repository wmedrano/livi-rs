use std::{collections::HashMap, sync::Arc};

use livi::lilv;

fn main() {
    let world = livi::World::new();
    let class_to_parent = make_class_to_parent_map(&world.raw().plugin_class().unwrap());
    for plugin in world.iter_plugins() {
        let classes = plugin_classes(&plugin, &class_to_parent);
        println!("{}: {:?}", plugin.name(), classes);
    }
}

fn plugin_classes(p: &livi::Plugin, class_to_parent: &HashMap<Class, Arc<Class>>) -> Vec<String> {
    let mut class = Class::from_raw(&p.raw().class());
    let mut ret = vec![class.name.clone()];
    while let Some(parent) = class_to_parent.get(&class) {
        class = parent.as_ref().clone();
        ret.push(class.name.clone());
    }
    ret
}

#[derive(Hash, Eq, PartialEq, Debug, Clone)]
struct Class {
    uri: String,
    name: String,
}

impl Class {
    pub fn from_raw(c: &lilv::plugin::Class) -> Class {
        Class {
            uri: c.uri().unwrap().turtle_token(),
            name: c.label().turtle_token(),
        }
    }
}

fn make_class_to_parent_map(top: &lilv::plugin::Class) -> HashMap<Class, Arc<Class>> {
    let mut ret = HashMap::new();
    populate_class_to_parent_children(top, Arc::new(Class::from_raw(top)), &mut ret);
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
