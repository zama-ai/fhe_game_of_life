use std::io::empty;
use rand::Rng;
use tfhe::prelude::*;
use tfhe::{generate_keys, set_server_key, ConfigBuilder, FheUint2};

const KEY_PATH: &str = "keys.bin";

use rayon::prelude::*;
use tfhe::shortint::{CarryModulus, MessageModulus};

fn is_alive(sks: &tfhe::shortint::ServerKey, cell: &tfhe::shortint::Ciphertext, neighbours: &[&tfhe::shortint::Ciphertext]) -> tfhe::shortint::Ciphertext {
    match (sks.message_modulus, sks.carry_modulus) {
        (MessageModulus(16), CarryModulus(1)) => {
            is_alive_4b(sks, cell, neighbours)
        }
        (MessageModulus(2), CarryModulus(8)) => {
            is_alive_4b(sks, cell, neighbours)
        }
        (MessageModulus(2), CarryModulus(16)) => {
            is_alive_5b(sks, cell, neighbours)
        }
        (MessageModulus(16), CarryModulus(2)) => {
            is_alive_5b(sks, cell, neighbours)
        }
        (MessageModulus(32), CarryModulus(1)) => {
            is_alive_5b(sks, cell, neighbours)
        }
        _ => {
            panic!("not supported")
        }
    }
}


/// Rules are
///
/// a live cell will survive if it has 2 or 3 neighbours alive
/// a dead cell will birth if it has 3 neighbours alive
fn is_alive_4b(sks: &tfhe::shortint::ServerKey, cell: &tfhe::shortint::Ciphertext, neighbours: &[&tfhe::shortint::Ciphertext]) -> tfhe::shortint::Ciphertext {
    let mut num_neighbours_alive = neighbours[0].clone();
    for n in neighbours[1..].iter() {
        sks.unchecked_add_assign(&mut num_neighbours_alive, n);
    }

    let lut1 = sks.generate_lookup_table(|x| {
        if x == 2 || x == 3 {
            x - 1
        } else {
            0
        }
    });

    sks.apply_lookup_table_assign(&mut num_neighbours_alive, &lut1);
    sks.unchecked_add_assign(&mut num_neighbours_alive, cell);

    let lut2 = sks.generate_lookup_table(|x| {
        // If x is 3, x was 2 prior to adding the cell value (sum of neigbours was 3)
        // then either:
        //  cell was 1: we are in the case where cell is alive with 2 neighbours so it continues
        //  cell was 0: we are in the case where original the sum of neighbours was 3, to the cell lives regardless
        // If x is 2, x was 1 prior to adding the cell value (sum of neighoburs was 2)
        // then either:
        //  cell was 1: we are in the case where cell is alive with 2 neighbours so it continues
        //  cell was 0: we are in the case where original the sum of neighbours was 3, to the cell lives regardless
        if x == 2 || x == 3 {
            1
        } else {
            0
        }
    });

    sks.apply_lookup_table_assign(&mut num_neighbours_alive, &lut2);

    num_neighbours_alive
}

fn is_alive_5b(sks: &tfhe::shortint::ServerKey, cell: &tfhe::shortint::Ciphertext, neighbours: &[&tfhe::shortint::Ciphertext]) -> tfhe::shortint::Ciphertext {
    assert!(sks.message_modulus.0 * sks.carry_modulus.0 >= 32);
    let mut num_neighbours_alive = neighbours[0].clone();
    for n in neighbours[1..].iter() {
        sks.unchecked_add_assign(&mut num_neighbours_alive, n);
    }

    let factor = 16;
    let shifted_cell = sks.scalar_mul(cell, factor);
    sks.unchecked_add_assign(&mut num_neighbours_alive, &shifted_cell);

    let lut1 = sks.generate_lookup_table(|x| {
        let cell = x / factor as u64;
        let num_n = x % factor as u64;
        u64::from(num_n == 3 || ((cell == 1) && num_n == 2))
    });
    sks.apply_lookup_table(&num_neighbours_alive, &lut1)
}


struct Board {
    dimensions: (usize, usize),
    states: Vec<tfhe::shortint::Ciphertext>,
    new_states: Vec<tfhe::shortint::Ciphertext>,
    // Indices used for task parallelism
    indices: Vec<(usize, usize)>,
    sks: tfhe::shortint::ServerKey,
}

impl Board {
    pub fn new(n_cols: usize, states: Vec<tfhe::shortint::Ciphertext>, sks: tfhe::shortint::ServerKey) -> Self {
        let n_rows = states.len() / n_cols;
        let n_elem = states.len();

        Self {
            dimensions: (n_rows, n_cols),
            states,
            new_states: Vec::with_capacity(n_elem),
            indices: itertools::iproduct!(0..n_rows, 0..n_cols).collect::<Vec<_>>(),
            sks,
        }
    }

    pub fn update(&mut self) {
        self.new_states.clear();

        let nx = self.dimensions.0;
        let ny = self.dimensions.1;

        self.indices.par_iter()
            .copied()
            // .zip(rayon::iter::repeatn(self.states.clone(), l))
            .map(|(i, j)| {

                let im = if i == 0 { nx - 1 } else { i - 1 };
                let ip = if i == nx - 1 { 0 } else { i + 1 };

                let jm = if j == 0 { ny - 1 } else { j - 1 };
                let jp = if j == ny - 1 { 0 } else { j + 1 };

                // get the neighbours, with periodic boundary conditions
                let n1 = &self.states[im * ny + jm];
                let n2 = &self.states[im * ny + j];
                let n3 = &self.states[im * ny + jp];
                let n4 = &self.states[i * ny + jm];
                let n5 = &self.states[i * ny + jp];
                let n6 = &self.states[ip * ny + jm];
                let n7 = &self.states[ip * ny + j];
                let n8 = &self.states[ip * ny + jp];

                // see if the cell is alive of dead
                is_alive(
                    &self.sks,
                    &self.states[i * ny + j],
                    &[n1, n2, n3, n4, n5, n6, n7, n8],
                )
        }).collect_into_vec(&mut self.new_states);

        // self.new_states = self.indices.iter()
        //     .copied()
        //     // .zip(rayon::iter::repeatn(self.states.clone(), l))
        //     .map(|(i, j)| {
        //
        //         let im = if i == 0 { nx - 1 } else { i - 1 };
        //         let ip = if i == nx - 1 { 0 } else { i + 1 };
        //
        //         let jm = if j == 0 { ny - 1 } else { j - 1 };
        //         let jp = if j == ny - 1 { 0 } else { j + 1 };
        //
        //         // get the neighbours, with periodic boundary conditions
        //         let n1 = &self.states[im * ny + jm];
        //         let n2 = &self.states[im * ny + j];
        //         let n3 = &self.states[im * ny + jp];
        //         let n4 = &self.states[i * ny + jm];
        //         let n5 = &self.states[i * ny + jp];
        //         let n6 = &self.states[ip * ny + jm];
        //         let n7 = &self.states[ip * ny + j];
        //         let n8 = &self.states[ip * ny + jp];
        //
        //         // see if the cell is alive of dead
        //         is_alive(
        //             &self.sks,
        //             &self.states[i * ny + j],
        //             &[n1, n2, n3, n4, n5, n6, n7, n8],
        //         )
        //     }).collect();

        // update the board
        std::mem::swap(&mut self.new_states, &mut self.states);
    }
}

fn main() {
    use std::time::Instant;
    let before = Instant::now();

    let (n_rows, n_cols): (usize, usize) = (20, 20);

    let keygen_start = Instant::now();
    // let param = tfhe::shortint::parameters::PARAM_MESSAGE_4_CARRY_0_KS_PBS;
    // let param = tfhe::shortint::parameters::PARAM_MESSAGE_1_CARRY_3_KS_PBS;
    // let param = tfhe::shortint::parameters::PARAM_MESSAGE_1_CARRY_4_KS_PBS;
    // let param = tfhe::shortint::parameters::PARAM_MESSAGE_5_CARRY_0_KS_PBS;
    let param = tfhe::shortint::parameters::PARAM_MESSAGE_4_CARRY_1_KS_PBS;
    let (cks, sks) = tfhe::shortint::gen_keys(param);
    println!("Key Generation time {:.3?}", keygen_start.elapsed());

    let states = if (n_rows, n_cols) == (6, 6) {
        // initial configuration
        #[rustfmt::skip]
            let states = vec![
            1, 0, 0, 0, 0, 0,
            0, 1, 1, 0, 0, 0,
            1, 1, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0,
            0, 0, 0, 0, 0, 0,
        ];
        states
    } else {
        let mut states = vec![0; n_rows * n_cols];
        let mut rng = rand::thread_rng();
        for s in &mut states {
            *s = rng.gen_range(0..=1);
        }
        states
    };

    // encrypt the initial configuration
    let states: Vec<_> = states
        .into_iter()
        .take(n_rows * n_rows)
        .map(|x| cks.encrypt(x))
        .collect();

    let mut board = Board::new(n_cols, states, sks);

    let mut count = 0;
    loop {
        print!("iter: {}", count);
        // show the board
        for i in 0..n_rows {
            println!();
            for j in 0..n_rows {
                if cks.decrypt(&board.states[i * n_cols + j]) != 0 {
                    print!("█");
                } else {
                    print!("░");
                }
            }
        }
        println!();

        // increase the time step
        let update_start = Instant::now();
        board.update();
        println!("Time to update: {:.3?}", update_start.elapsed());

        count += 1;
        if count == 5 {
            break;
        }
    }

    println!("Elapsed time: {:.3?}", before.elapsed());
}
