use tfhe::prelude::*;
use tfhe::{generate_keys, set_server_key, ConfigBuilder, FheUint2};

const KEY_PATH: &str = "keys.bin";

use rayon::prelude::*;

/// Rules are
///
/// a live cell will survive if it has 2 or 3 neighbours alive
/// a dead cell will birth if it has 3 neighbours alive
fn is_alive(cell: &FheUint2, neighbours: &[&FheUint2]) -> FheUint2 {
    let mut num_neighbours_alive = FheUint2::try_encrypt_trivial(0).unwrap();
    for n in neighbours {
        num_neighbours_alive += *n;
    }
    // The above could be written as:
    // let num_neighbours_alive: FheUint2 = neighbours.into_iter().copied().sum();


    num_neighbours_alive.bivariate_function(cell, |n, c| {
        u8::from((n == 3) || (c == 1 && (n == 2)))
    })
    // num_neighbours_alive.eq(3) | (cell & num_neighbours_alive.eq(2))
}

struct Board {
    dimensions: (usize, usize),
    states: Vec<FheUint2>,
    new_states: Vec<FheUint2>,
}

impl Board {
    pub fn new(n_cols: usize, states: Vec<FheUint2>) -> Self {
        let n_rows = states.len() / n_cols;
        let n_elem = states.len();

        Self {
            dimensions: (n_rows, n_cols),
            states,
            new_states: Vec::with_capacity(n_elem),
        }
    }

    pub fn update(&mut self) {
        self.new_states.clear();

        let nx = self.dimensions.0;
        let ny = self.dimensions.1;

        // (0..nx).into_par_iter().map(|i| {
        //     (0..ny).into_iter()
        //         .map()
        // })

        let jobs = itertools::iproduct!(0..nx, 0..ny).collect::<Vec<_>>();

        let l = jobs.len();
        let lol = jobs.into_par_iter()
            .zip(rayon::iter::repeatn(self.states.clone(), l))
            .map(|((i, j), states)| {

                let im = if i == 0 { nx - 1 } else { i - 1 };
                let ip = if i == nx - 1 { 0 } else { i + 1 };

                let jm = if j == 0 { ny - 1 } else { j - 1 };
                let jp = if j == ny - 1 { 0 } else { j + 1 };

                // get the neighbours, with periodic boundary conditions
                let n1 = &states[im * ny + jm];
                let n2 = &states[im * ny + j];
                let n3 = &states[im * ny + jp];
                let n4 = &states[i * ny + jm];
                let n5 = &states[i * ny + jp];
                let n6 = &states[ip * ny + jm];
                let n7 = &states[ip * ny + j];
                let n8 = &states[ip * ny + jp];

                // see if the cell is alive of dead
                is_alive(
                    &states[i * ny + j],
                    &[n1, n2, n3, n4, n5, n6, n7, n8],
                )
        }).collect::<Vec<_>>();
        self.new_states = lol;

        // for i in 0..nx {
        //     let im = if i == 0 { nx - 1 } else { i - 1 };
        //     let ip = if i == nx - 1 { 0 } else { i + 1 };
        //     for j in 0..ny {
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
        //         self.new_states.push(is_alive(
        //             &self.states[i * ny + j],
        //             &[n1, n2, n3, n4, n5, n6, n7, n8],
        //         ));
        //     }
        // }

        // update the board
        std::mem::swap(&mut self.new_states, &mut self.states);
    }
}

fn main() {
    use std::time::Instant;
    let before = Instant::now();

    let (n_rows, n_cols): (usize, usize) = (6, 6);

    let keygen_start = Instant::now();
    let config = ConfigBuilder::all_disabled().enable_default_uint2().build();
    let (client_key, server_key) = generate_keys(config);
    println!("Key Generation time {:.3?}", keygen_start.elapsed());

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

    // encrypt the initial configuration
    let states: Vec<_> = states
        .into_iter()
        .map(|x| FheUint2::try_encrypt(x, &client_key).unwrap())
        .collect();

    set_server_key(server_key.clone());
    rayon::broadcast(|_| set_server_key(server_key.clone()));

    let mut board = Board::new(n_cols, states);

    let mut count = 0;
    loop {
        print!("iter: {}", count);
        // show the board
        for i in 0..n_rows {
            println!();
            for j in 0..n_rows {
                if (&board.states[i * n_cols + j]).decrypt(&client_key) != 0 {
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
