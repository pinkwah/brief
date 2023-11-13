use std::cmp::max;

#[derive(Default)]
pub struct Table {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl Table {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_header(&mut self, text: String) {
        self.headers.push(text);
    }

    pub fn add_row(&mut self, row: Vec<String>) {
        self.rows.push(row);
    }

    pub fn print(&self) {
        let colsizes: Vec<usize> = self
            .headers
            .iter()
            .enumerate()
            .map(|(col, head)| {
                max(
                    self.rows
                        .iter()
                        .map(|row| row.get(col).map(|cell| cell.len()).unwrap_or(0))
                        .max()
                        .unwrap(),
                    head.len(),
                )
            })
            .collect();

        // Print head
        for (i, head) in self.headers.iter().enumerate() {
            print!("{:len$}\t", head, len = colsizes.get(i).unwrap_or(&0));
        }
        println!();

        // Print rows
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                print!("{:len$}\t", cell, len = colsizes.get(i).unwrap_or(&0));
            }
            println!();
        }
    }
}
