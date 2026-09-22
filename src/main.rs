use std::thread ;

fn main() {

    let data = vec![1,2,3] ;

    thread::scope(|s| {
        s.spawn(|| {
            let (idx1, idx2) = (0, 2) ;
            println!("[{}..{}] -> {:?}", idx1, idx2, &data[idx1..idx2]) ;   // Out: [0..2] -> [1, 2]
        }) ;

        s.spawn(|| {
            let idx1 = 0 ;
            println!("[{}..] -> {:?}", idx1, &data[idx1..]) ;   // [0..] -> [1, 2, 3]
        }) ;
    }) ;

    // data - доступен
    println!("{:?}", data) ;    // [1, 2, 3]
}
