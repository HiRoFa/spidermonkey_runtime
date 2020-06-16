use std::collections::HashMap;

/// AutoIdMap is a wrapper around HashMap which automatically creates a unique id for it's entries
/// # Example
/// ```no_run
/// use es_runtime::utils::AutoIdMap;
///
/// let mut map = AutoIdMap::new();
/// let id1 = map.insert("hi");
/// let id2 = map.insert("hi2");
/// assert_ne!(id1, id2);
/// assert_eq!(map.len(), 2);
/// let s1 = map.remove(&id1);
/// assert_eq!(s1, "hi");
/// assert_eq!(map.len(), 1);
/// ```
pub struct AutoIdMap<T> {
    last_id: usize,
    map: HashMap<usize, T>,
}

impl<T> AutoIdMap<T> {
    /// create a new instance of the AutoIdMap
    pub fn new() -> AutoIdMap<T> {
        AutoIdMap {
            last_id: 0,
            map: HashMap::new(),
        }
    }

    /// insert an element and return the new id
    pub fn insert(&mut self, elem: T) -> usize {
        self.last_id += 1;
        self.map.insert(self.last_id, elem);
        self.last_id
    }

    /// replace an element, this will panic if you pas san id that is not present
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn replace(&mut self, id: &usize, elem: T) {
        // because we really don't want you to abuse this to insert your own id's :)
        if !self.contains_key(id) {
            panic!("no entry to replace for {}", id);
        }
        self.map.insert(*id, elem);
    }

    /// get an element base don it's id
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn get(&self, id: &usize) -> Option<&T> {
        self.map.get(id)
    }

    /// remove an element based on its id
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn remove(&mut self, id: &usize) -> T {
        self.map.remove(id).expect("no such elem")
    }

    /// get the size of the map
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.map.len()
    }

    /// see if map is empty
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.map.is_empty()
    }

    /// check if a map contains a certain id
    #[allow(clippy::trivially_copy_pass_by_ref)]
    pub fn contains_key(&self, id: &usize) -> bool {
        self.map.contains_key(id)
    }
}

impl<T> Default for AutoIdMap<T> {
    fn default() -> Self {
        AutoIdMap::new()
    }
}
