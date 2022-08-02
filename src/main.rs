use concrete_boolean::ciphertext::Ciphertext;
use concrete_boolean::gen_keys;
use concrete_boolean::server_key::ServerKey;

// 3-bits accumulator
// The value 8 is identified with 0.
fn add_1(
    server_key: &ServerKey,
    a: &Ciphertext,
    b: &(Ciphertext, Ciphertext, Ciphertext),
) -> (Ciphertext, Ciphertext, Ciphertext) {
    // lowest bit of the result
    let c1 = server_key.xor(a, &b.0);

    // first carry
    let r = server_key.and(a, &b.0);

    // second lowest bit of the result
    let c2 = server_key.xor(&r, &b.1);

    // second carry
    let r = server_key.and(&r, &b.1);

    // highest bit of the result
    let c3 = server_key.xor(&r, &b.2);

    (c1, c2, c3)
}

fn sum(
    server_key: &ServerKey,
    elements: &[&Ciphertext],
    zeros: (Ciphertext, Ciphertext, Ciphertext),
) -> (Ciphertext, Ciphertext, Ciphertext) {
    let mut result = zeros;
    for i in 0..elements.len() {
        result = add_1(server_key, elements[i], &result);
    }
    result
}

fn is_alive(
    server_key: &ServerKey,
    cell: &Ciphertext,
    neighbours: &[&Ciphertext],
    zeros: &(Ciphertext, Ciphertext, Ciphertext),
) -> Ciphertext {
    // perform the sum
    let sum_neighbours = sum(server_key, neighbours, zeros.clone());

    // check if the sum is equal to 2 or 3
    let sum_is_2_or_3 = server_key.and(&sum_neighbours.1, &server_key.not(&sum_neighbours.2));

    // check if the sum is 3
    let sum_is_3 = server_key.and(
        &sum_neighbours.0,
        &server_key.and(&sum_neighbours.1, &server_key.not(&sum_neighbours.2)),
    );

    // return (an encryption of) the new state of the cell
    server_key.or(&sum_is_3, &server_key.and(cell, &sum_is_2_or_3))
}

/// a board structure for Conway's game of Life
///
/// Fields:
///
///  dimensions: the height and width of the board
///  states: vector of ciphertextx encoding the current state of each cell
pub struct Board {
    dimensions: (usize, usize),
    states: Vec<Ciphertext>,
}

impl Board {
    /// build a new board
    ///
    /// Arguments:
    ///
    ///  n_cols: the number of columns of the board
    ///  states: vector of ciphertexts encoding the initial state of each cell
    ///
    /// If the length of the states vector is not a multiplt of n_cols, the cells on the incomplete
    /// row will not be updated.
    pub fn new(n_cols: usize, states: Vec<Ciphertext>) -> Board {
        // compute the number of rows
        let n_rows = states.len() / n_cols;

        Board {
            dimensions: (n_rows, n_cols),
            states,
        }
    }

    /// update the state of each cell
    ///
    /// Arguments:
    ///
    ///  server_key: the server key needed to perform homomorphic operations
    ///  zeros: three encryptions of false (which may be identical)
    pub fn update(&mut self, server_key: &ServerKey, zeros: &(Ciphertext, Ciphertext, Ciphertext)) {
        let mut new_states = Vec::<Ciphertext>::new();

        let nx = self.dimensions.0;
        let ny = self.dimensions.1;
        for i in 0..nx {
            let im = if i == 0 { nx - 1 } else { i - 1 };
            let ip = if i == nx - 1 { 0 } else { i + 1 };
            for j in 0..ny {
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
                new_states.push(is_alive(
                    server_key,
                    &self.states[i * ny + j],
                    &vec![n1, n2, n3, n4, n5, n6, n7, n8],
                    zeros,
                ));
            }
        }

        // update the board
        self.states = new_states;
    }
}

fn main() {
    use std::time::Instant;
    let before = Instant::now();

    // define the board dimensions
    let (n_rows, n_cols): (usize, usize) = (6, 6);

    // generate the client and server keys
    let (client_key, server_key) = gen_keys();

    // compute three encryption of 0
    // (we could also work with only one; but this is quite fast in practice)
    let zeros = (
        client_key.encrypt(false),
        client_key.encrypt(false),
        client_key.encrypt(false),
    );

    // initial configuration
    #[rustfmt::skip]
    let states = vec![
        true, false, false, false, false, false,
        false, true, true, false, false, false,
        true, true, false, false, false, false,
        false, false, false, false, false, false,
        false, false, false, false, false, false,
        false, false, false, false, false, false,
    ];

    // encrypt the initial configuration
    let states: Vec<Ciphertext> = states.into_iter().map(|x| client_key.encrypt(x)).collect();

    // build the board
    let mut board = Board::new(n_cols, states);

    let mut count = 0;
    loop {
        print!("iter: {}", count);
        // show the board
        for i in 0..n_rows {
            println!();
            for j in 0..n_rows {
                if client_key.decrypt(&board.states[i * n_cols + j]) {
                    print!("█");
                } else {
                    print!("░");
                }
            }
        }
        println!();

        // increase the time step
        board.update(&server_key, &zeros);
        count += 1;
        if count == 5 {
            break;
        }
    }

    println!("Elapsed time: {:.2?}", before.elapsed());
}
