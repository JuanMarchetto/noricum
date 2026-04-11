use std::collections::VecDeque;

const INITIAL_CAPACITY: usize = 8;
const LOAD_FACTOR_THRESHOLD: u32 = 75;

#[derive(Debug, Clone)]
struct Entry {
    key: String,
    value: i32,
}

#[derive(Debug)]
struct HashTable {
    buckets: Vec<VecDeque<Entry>>,
    size: usize,
}

impl HashTable {
    fn new() -> Self {
        let mut buckets = Vec::with_capacity(INITIAL_CAPACITY);
        for _ in 0..INITIAL_CAPACITY {
            buckets.push(VecDeque::new());
        }
        
        HashTable {
            buckets,
            size: 0,
        }
    }
    
    fn hash_key(key: &str) -> usize {
        let mut hash: usize = 5381;
        for c in key.bytes() {
            hash = hash.wrapping_shl(5).wrapping_add(hash).wrapping_add(c as usize);
        }
        hash
    }
    
    fn resize(&mut self, new_capacity: usize) {
        let mut new_buckets = Vec::with_capacity(new_capacity);
        for _ in 0..new_capacity {
            new_buckets.push(VecDeque::new());
        }
        
        for bucket in self.buckets.drain(..) {
            for entry in bucket {
                let idx = Self::hash_key(&entry.key) % new_capacity;
                new_buckets[idx].push_front(entry);
            }
        }
        
        self.buckets = new_buckets;
    }
    
    fn set(&mut self, key: &str, value: i32) -> Result<(), &'static str> {
        if (self.size * 100).checked_div(self.buckets.len())
            .map_or(false, |load| load >= LOAD_FACTOR_THRESHOLD as usize) {
            self.resize(self.buckets.len() * 2);
        }
        
        let idx = Self::hash_key(key) % self.buckets.len();
        
        for entry in &mut self.buckets[idx] {
            if entry.key == key {
                entry.value = value;
                return Ok(());
            }
        }
        
        self.buckets[idx].push_front(Entry {
            key: key.to_string(),
            value,
        });
        self.size += 1;
        
        Ok(())
    }
    
    fn get(&self, key: &str) -> Option<i32> {
        let idx = Self::hash_key(key) % self.buckets.len();
        
        self.buckets[idx]
            .iter()
            .find(|entry| entry.key == key)
            .map(|entry| entry.value)
    }
    
    fn delete(&mut self, key: &str) -> Result<(), &'static str> {
        let idx = Self::hash_key(key) % self.buckets.len();
        
        if let Some(pos) = self.buckets[idx]
            .iter()
            .position(|entry| entry.key == key) {
            self.buckets[idx].remove(pos);
            self.size -= 1;
            Ok(())
        } else {
            Err("Key not found")
        }
    }
}

fn main() {
    let mut ht = HashTable::new();
    let mut val = 0;
    
    ht.set("alpha", 1).unwrap();
    ht.set("beta", 2).unwrap();
    ht.set("gamma", 3).unwrap();
    println!("size={}", ht.size);
    
    if let Some(v) = ht.get("beta") {
        val = v;
        println!("beta={}", val);
    }
    
    ht.set("beta", 42).unwrap();
    if let Some(v) = ht.get("beta") {
        val = v;
        println!("beta_updated={}", val);
    }
    
    ht.delete("alpha").unwrap();
    println!("after_delete={}", ht.size);
    
    let found = ht.get("alpha");
    println!("alpha_found={}", if found.is_some() { 0 } else { -1 });
    
    for i in 0..20 {
        let key = format!("key_{}", i);
        ht.set(&key, i * 10).unwrap();
    }
    println!("after_bulk={}", ht.size);
    
    if let Some(v) = ht.get("key_0") {
        val = v;
        println!("key_0={}", val);
    }
    if let Some(v) = ht.get("key_19") {
        val = v;
        println!("key_19={}", val);
    }
    
    println!("done");
}