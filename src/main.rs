use concrete::prelude::*;
use concrete::{generate_keys, set_server_key, ConfigBuilder, FheBool};
use std::ops::AddAssign;

#[derive(Clone)]
struct Accumulator(FheBool, FheBool, FheBool);

impl From<(FheBool, FheBool, FheBool)> for Accumulator {
    fn from(inner: (FheBool, FheBool, FheBool)) -> Self {
        Self(inner.0, inner.1, inner.2)
    }
}

impl AddAssign<&FheBool> for Accumulator {
    // ^ -> xor
    // & -> and
    fn add_assign(&mut self, rhs: &FheBool) {
        let c1 =  &self.0 ^ rhs; 
        let first_carry = rhs & &self.0;

        let second_carry = &first_carry & &self.1;
        let c2 = &self.1 ^ first_carry;

        self.2 = &self.2 ^ second_carry;
        self.1 = c2;
        self.0 = c1;
    }
}

/// Rules are
///
/// a live cell will survive if it has 2 or 3 neighbours alive
/// a dead cell will birth if it has 3 neighbours alive
fn is_alive(cell: &FheBool, neighbours: &[&FheBool], mut accumulator: Accumulator) -> FheBool {
    for neighbour in neighbours {
        accumulator += *neighbour;
    }

    // check if sum is equal to 2 or 3
    let sum_is_2_or_3 = !accumulator.2 & accumulator.1;
    let sum_is_3 = &sum_is_2_or_3 & accumulator.0;

    sum_is_3 | cell & sum_is_2_or_3
}

struct Board {
    dimensions: (usize, usize),
    states: Vec<FheBool>,
    clean_accumulator: Accumulator,
}

impl Board {
    pub fn new(n_cols: usize, states: Vec<FheBool>, zeros: (FheBool, FheBool, FheBool)) -> Self {
        let n_rows = states.len() / n_cols;

        Self {
            dimensions: (n_rows, n_cols),
            states,
            clean_accumulator: Accumulator::from(zeros),
        }
    }

    pub fn update(&mut self) {
        let mut new_states = Vec::<FheBool>::with_capacity(self.states.len());

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
                    &self.states[i * ny + j],
                    &vec![n1, n2, n3, n4, n5, n6, n7, n8],
                    self.clean_accumulator.clone(),
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

    let (n_rows, n_cols): (usize, usize) = (6, 6);

    let config = ConfigBuilder::all_disabled().enable_default_bool().build();

    let (client_key, server_key) = generate_keys(config);

    let zeros = (
        FheBool::encrypt(false, &client_key),
        FheBool::encrypt(false, &client_key),
        FheBool::encrypt(false, &client_key),
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
    let states: Vec<_> = states
        .into_iter()
        .map(|x| FheBool::encrypt(x, &client_key))
        .collect();

    set_server_key(server_key);

    let mut board = Board::new(n_cols, states, zeros);

    let mut count = 0;
    loop {
        print!("iter: {}", count);
        // show the board
        for i in 0..n_rows {
            println!();
            for j in 0..n_rows {
                if (&board.states[i * n_cols + j]).decrypt(&client_key) {
                    print!("█");
                } else {
                    print!("░");
                }
            }
        }
        println!();

        // increase the time step
        board.update();
        count += 1;
        if count == 5 {
            break;
        }
    }

    println!("Elapsed time: {:.2?}", before.elapsed());
}

#[cfg(test)]
mod tests {
    use crate::Accumulator;
    use concrete::prelude::*;
    use concrete::{generate_keys, ConfigBuilder, FheBool};

    fn decrypt_acc(acc: &Accumulator, keys: &mut KeyChain) -> (FheBool, FheBool, FheBool) {
        (
            acc.2.to_FheBool(keys),
            acc.1.to_FheBool(keys),
            acc.0.to_FheBool(keys),
        )
    }

    #[test]
    fn test_accumulator() {
        let config = ConfigBuilder::all_disabled().enable_default_bool().build();

        let (client_key, server_key) = generate_keys(config);

        let zeros = (
            FheBool::encrypt(false, &client_key),
            FheBool::encrypt(false, &client_key),
            FheBool::encrypt(false, &client_key),
        );

        let mut accumulator = Accumulator::from(zeros);
        let ftrue = FheBool::encrypt(true, &mut keys);
        let ffalse = FheBool::encrypt(false, &mut keys);

        let bits = decrypt_acc(&accumulator, &mut keys);
        assert_eq!(bits, (false, false, false));

        accumulator += &ftrue;
        let bits = decrypt_acc(&accumulator, &mut keys);
        assert_eq!(bits, (false, false, true));
    }
}
