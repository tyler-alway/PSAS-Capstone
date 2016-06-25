use std::net::UdpSocket;
use time::precise_time_s;

extern crate libs;
extern crate libc;
extern crate time;

mod control;
mod sensor;
mod data_fmt;

// Add: use control_interface::Control; soon
// Shared memory structure 
pub struct SharedMemory {
    gyro_x:    f32,
    gyro_y:    f32,
    gyro_z:    f32,
    cw_state:  u8,
    ccw_state: u8,
}

fn main() {
    println!("main function\n");

    let mut mem = SharedMemory{gyro_x: 0.0, gyro_y: 0.0, gyro_z: 0.0,
                               cw_state: 0, ccw_state: 0};

    // Timestep variables
    let Hz :f64 = 2.0;  // Define the Hz to be used -- Using 2 Hz for testing
    let expected_timestep = 1.0/Hz; // Inverse of frequency
    let mut running = true;
    let mut previous_time;
    let mut current_time = precise_time_s();
    let mut time_since_last : f64 = 0.0;

    sensor::init(); // Replace with let mut sen = sensor::init(&mut mem); soon
    control::init();        // Replace with let mut ctl = control::init(); soon

    let mut socket = UdpSocket::bind("0.0.0.0:0").unwrap(); // Update with correct IP/Port later

    while running{
        // Update time variables
        previous_time = current_time;
        current_time = precise_time_s();
        time_since_last = time_since_last + current_time-previous_time;

        while time_since_last >= expected_timestep {
          if sensor::update(&mut mem)==1{ // Replace with sen.update(&mut mem); soon
            println!("Error during sensor update.\n");
            running = false;
            break;
          }
          if control::update(&mut mem)==1{
            println!("Error during control update.\n");
            running = false;
            break;
          }
          match data_fmt::send_packet(&socket, &mem){
            1=>{
            println!("Error during data formatter send_packet.\n");
            running = false;
            break;}
            2=>{
            println!("Exception during data formatter send_packet.\n");
            running = false;
            break;
            }
            _=>()
          }
          // Decrease by expected timestep
          time_since_last -= expected_timestep;
          println!("\n"); // Remove this when done testing otherwise outputting to console is a bottleneck
        }

    }
}

// Run as: cargo test -- --nocapture to see useful output about cycles
#[test]
fn timestep(){
    let Hz :f64 = 2.0;  // Define the HZ to be used 
    let mut freq = 0;
    let mut cycles : f64 = 0.0;

    let expected_timestep = 1.0/Hz; // Inverse of frequency
    let mut running = true;
    let mut previous_time;
    let mut current_time = precise_time_s();
    let mut time_since_last : f64 = 0.0;
    let mut elapsed_time= precise_time_s()- precise_time_s();


    while elapsed_time<=10.0{ // Run for 10 seconds        
        // Update time variables
        previous_time = current_time;
        current_time = precise_time_s();
        time_since_last = time_since_last + current_time-previous_time;
        elapsed_time += current_time-previous_time;
        while time_since_last >= expected_timestep {
        println!("Cycles: {}", cycles);
          cycles+=1.0;
          // Decrease by expected timestep
          time_since_last -= expected_timestep;
        }
    }
    println!("Cycles: {}", cycles);
    println!("Time: {}", elapsed_time);
    assert_eq!(within(0.0001, cycles/(elapsed_time), Hz), true); // Accept if the frequency is within .0001 Hz
}

// Since time libraries can only be so precice, I use this to give a little bit of error
fn within(error : f64, value : f64, expected : f64) -> bool{
    if((value)<expected+error&&(value)>expected-error){
        return true;
    }
    return false;
}