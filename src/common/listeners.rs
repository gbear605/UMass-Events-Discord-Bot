use std::fs::OpenOptions;
use std::io::Read;

pub fn read_listeners_generic<T>(file_name: &str, f: &dyn Fn(String) -> T) -> Vec<(T, String)> {
    let mut listeners_string: String = String::new();
    let _ = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .open(file_name)
        .expect("No listeners file")
        .read_to_string(&mut listeners_string);

    let mut listeners: Vec<(T, String)> = vec![];

    for line in listeners_string.split('\n') {
        if line == "" {
            continue;
        }
        let sections: Vec<&str> = line.split(' ').collect();
        let id = f(sections[1].to_string());

        let food: String = sections[2..].join(" ").to_string();
        listeners.push((id, food));
    }

    listeners
}
