# Reaction Control System (RCS) Software Design

## Overview

The Reaction Control System (RCS) software can be compiled for two distinct
target modes -- flight mode and test mode. Common components to both flight and
test modes include Main, Control, Sensor, and Data Formatter modules.

In flight mode, sensor data is retrieved from the hardware over an I2C bus
using Rust libraries. The control algorithm uses sensor data to
compute any required changes in trajectory and again uses Rust
libraries to assert signals on the hardware GPIO pins to control the rocket
nozzles.

In test mode, the Rust libraries are replaced by a controller/sensor
interface that matches the library interface used in flight mode.
The hardware is replaced by JSBSim to model sensor responses to control inputs.

## Software Components

### _Common Components_
#### Main
The main module is executed at program startup and does the following:

```
PRINT startup information including whether test mode is enabled

INIT Running TO true
INIT struct containing to-be-determined information which will be shared between modules
CALL InitializeSensorModule
CALL InitializeControlModule
INIT variables/class/struct containing means of using high-precision time constructs
     to perform a fixed timestep loop
INIT UDP_Socket connection

WHILE Running EQUAL true
    SET TimeConstruct.CurrentTime TO CALL RustLibraryGetCurrentTime
    INCREMENT TimeConstruct.TimeSinceLastUpdate BY TimeConstruct.CurrentTime
                                                - TimeConstruct.PreviousTime
    SET TimeConstruct.PreviousTime TO TimeConstruct.CurrentTime;
    WHILE TimeConstruct.TimeSinceLastUpdate >= constant_time_step
        IF CALL SensorModuleUpdate with reference to shared memory EQUAL 1 THEN
            PRINT information about error
            SET Running TO false
        ENDIF
        IF CALL ControlModuleUpdate with reference to shared memory EQUAL 1 THEN
            PRINT information about error
            SET Running TO false
        ENDIF
        CALL Data_Formatter::send_packet with reference to shared memory, UDP_Socket EQUAL 1 or 2 THEN
            PRINT information about error
            SET Running TO false
        ENDIF
        INCREMENT TimeConstruct.TimeSinceLastUpdate BY NEGATE constant_time_step;
    ENDWHILE
ENDWHILE

PRINT information about the RCS run
READ wait for user input to terminate program
```

#### Control Module
The control module implements the control algorithm. Sensor data is retrieved from shared memory and GPIO pins are asserted for course correction. This control design is based on [Gain_v3.py](https://github.com/psas/reaction-control/blob/master/Controller_Scripts/Gain_v3.py).

```
FUNCTION init()
    INPUTS: None
    OUTPUTS: Returns an initialized Control object

    STORE state <- 0 // control state variable

    // initialize gpio pins
    // Use pin 53 as clockwise (CW)
    // Use pin 54 as counter clockwise (CCW)
    // Use pin 0 as emergency stop (ESTOP)

    INITIALIZE CW_pin as an output and set its value to 0
    INITIALIZE CCW_pin as an output and set its value to 0
    INITIALIZE ESTOP_pin as an input

    RETURN Control object

END FUNCTION


FUNCTION update(mem)
    INPUTS:  mem -- reference to shared memory
    OUTPUTS: Returns 0 -- all is well
                     1 -- Shut Down! (HW asserted the emergency stop pin)

    stop_pin <- Get the value of the ESTOP pin
    IF stop_pin is 1
        RETURN 1
    END IF

    rate_x <- READ the gyro's x axis rate from shared memory

    IF rate_x GE 0.175
        CALL state_update(rate_x)
        CALL write_pin(CW_pin, new state value, mem)
    ELSE IF gyro_x LE -0.175
        CALL state_update(rate_x)
        CALL write_pin(CCW_pin, new state value, mem)
    ELSE
        // turn off both gpio pins
        CALL write_pin(CW_pin, 0, mem)
        CALL write_pin(CCW_pin, 0, mem)
    END IF

    RETURN 0
END FUNCTION


FUNCTION state_update(rate_x)
    INPUTS:  rate_x -- rotational rate about the x axis
    OUTPUTS: Writes new value to the control state

    // Wish the variables names were more descriptive here, but that's what
    // they are called in Gain_v3.py (see link above) ... don't want to make
    // any wrong assumptions that make it worse.

    kp <- 0.25     // proportional gain for duty cycle
    a <- 2.0 * kp  // (I/(r*.1s))/Ftot equation to dc from radian error
    u <- a*abs(rate_x)

    IF u GE 1.0
        state <- 1
    ELSE IF u LT 0.1
        state <- 0
    ELSE
        Toggle the state value
    END IF

END FUNCTION


FUNCTION write_pin(pin, value, mem)
    INPUTS:  pin -- the GPIO pin number (must be an output pin)
             value -- the value to write to the pin (0 or 1)
             mem -- reference to shared memory
    OUTPUTS: Returns 0 -- all is well
                     1 -- Error

    SET the pin's state to value

    STORE the pin's state in shared memory

    RETURN 0 if everything went well, 1 otherwise

END FUNCTION
```

#### Sensor Module
The sensor module retrieves sensor data and stores it in shared memory.  The sensor module provides:

* An initialization function that receives the location of shared memory and sets up the sensor hardware
* An update function that reads sensor data from hardware and stores it in shared memory

```

FUNCTION init()
    INPUTS: address of shared memory
    OUTPUTS: 0 -- all is well
             1 -- Error

    CALL myi2c <- i2c::init()

ENDFUNCTION


FUNCTION update(sharedMem: &mut SharedMemory)
    INPUTS: address of shared memory
    OUTPUTS: 0 -- all is well
             1 -- Error

    let mut buf = [0u8; (3 + 1 + 3) * 2]  // 3 accel (Registers 3b-40),
                                          // 1 temp (Registers 41-42), 3 gyro (Registers 43-48)

    CALL myi2c.write(0x3b) // 0x3b is the beginning address of the block of registers that we want to read
    CALL myi2c.read(&buf) // puts block (buf.length) of registers in buf (accel, temp, and gyro)

    WRITE buf into Shared Memory

ENDFUNCTION

```

#### Data Formatter
The data formatter take in data stored in Shared Memory, and serializes the UDP message it into a byte array. The header for the UDP packet is also serialized, then both the header and message byte arrays are stored in a Telemetry_buffer also stored in Shared memory. Once the Telemetry_buffer reaches a specified threshold, the packet is sent to the telemetry viewer. 

The PSAS telemetry viewer and the PSAS packet serializer must be installed to view the data sent by the Data Formatter - these are found in the following repositories.
https://github.com/psas/telemetry
https://github.com/psas/psas_packet
```
FUNCTION pack_header(name, time, packet_message_size)
    INPUTS: name                -- ASCII PSAS message definition name.
            time                -- Time duration with respect to boot time.
            packet_message_size -- Size of message associated with this header type.
            buffer              -- Buffer/byte array to hold header information.

    OUTPUTS: Ok(0)          -- all is well
             Err(io::Error) -- an error occurred
    
    
    SET Curser to move through Byte_array
    WRITE header name using ASCII code into Temp_buffer
    WRITE Timestamp into Temp_buffer
    WRITE size of packet message into Temp_buffer
    
    RETURN 0
    
END FUNCTION


FUNCTION as_message(addr)
    INPUTS: mem    --  Shared memory address
            buffer -- Buffer/byte array to hold message information.
            
    OUTPUTS: Ok(0)          -- all is well
             Err(io::Error) -- an error occurred
                     
                     
    SET Curser to move through Byte_array
    WRITE data fields for RCSS packet in shared memory into Temp_buffer
    
    RETURN 0
    
END FUNCTION


FUNCTION flush_telemetry(socket, addr)
    INPUTS: socket -- Socket binding
            mem    -- Shared memory address
    
    OUTPUTS: Ok(0)          -- all is well
             Err(io::Error) -- an error occurred
                    
                     
    SET address for UDP packet target
    SEND UDP packet containing Telemetry_buffer to the UDP packet target
    CLEAR Telemetry_buffer
    
    RETURN 0

END FUNCTION


FUNCTION send_packet(Socket, addr)
    INPUTS: socket -- Socket binding
            mem    -- Shared memory address
    
    OUTPUTS: Ok(0)          -- all is well
             Err(io::Error) -- an error occurred
    
    GET Timestamp for packet
    
    IF Telemetry_Buffer is full THEN
        CALL flush_telemetry
    ElSE IF Telemetry_buffer is empty
        APPEND packet sequence number to telemtry_buffer    
    ELSE
        CALL pack_header([u8; 4], time::Duration, usize)
        APPEND header to buffer
        CALL as_message(&mut SharedMemory) 
        APPEND message to buffer

    RETURN 0

END FUNCTION
```

### _Flight Mode Components_
#### Embedded Rust Libraries
During flight mode, the system reads sensor input and dispatches control signals via [I2C](https://github.com/rust-embedded/rust-i2cdev) and [GPIO](https://github.com/rust-embedded/rust-sysfs-gpio) Rust libraries.

##### I2C
```
//i2c File, accessible with i2c::init()
// Import libraries
extern crate i2cdev;
use i2cdev::*;

FUNCTION init()
   INPUTS: none
   OUTPUTS: Returns returns a Myi2c object

   // embedded linux libraries found here:
   // https://github.com/rust-embedded/rust-i2cdev.git
   Set up the i2c_device hardware // Refer to Jamey code for this
   RETURN LinuxI2CDevice
END FUNCTION

FUNCTION read(buf: &mut [u8])
   INPUTS: Memory the the data will be read into
   OUTPUTS: 0 -- all is well
            1 -- Error

   CALLS read from the rust library.
END FUNCTION

FUNCTION write(reg: &[u8])
   INPUTS: registers to write to
   OUTPUTS: 0 -- all is well
            1 -- Error
   CALLS write from the rust library.
END FUNCTION
```
##### GPIO

```
FUNCTION new()
    INPUTS: None
    OUTPUTS: Returns an object with an empty container

    Create a GPIO pins object that has an empty container

    RETURN GPIO pins object
END FUNCTION


FUNCTION add_pin(pin_number, direction)
    INPUTS: pin_number -- the desired GPIO pin number
            direction  -- input or output
    OUTPUTS: Errors if direction is invalid or underlying library has problems.

    Create a new pin using the pin number and direction as inputs to the embedded GPIO library.
    Add the pin to the container.
END FUNCTION


FUNCTION get_value(pin_number)
    INPUTS: pin_number -- the desired GPIO pin number
    OUTPUTS: Returns the pin's state value, or
             Reports an error if: 1) the pin was not initialized
                                  2) the underlying library has problems

    Look through the container for the pin that matches pin_number.
    If the pin doesn't exist in the container, report an error.
    Otherwise, call the embedded GPIO library to retrieve the pin's state.

    RETURN the pin's state
END FUNCTION


FUNCTION set_value(pin_number, value)
    INPUTS: pin_number -- the desired GPIO pin number
            value -- 0 to set the pin low, 1 to set the pin high
    OUTPUTS: Sets the pin's state value, or
             Reports an error if: 1) the pin was not initialized
                                  2) the pin is configured as an input
                                  3) the underlying library has problems

    Look through the container for the pin that matches pin_number.
    If the pin doesn't exist in the container, report an error.
    If the pin is configured as an input, report an error.
    Otherwise, call the embedded GPIO library to set the pin's state to value.

    RETURN okay status or error
END FUNCTION

```

### _Test Mode Components_
#### Controller Interface
The controller interface provides a set of functions that are equivalent to
the function calls made by the control module in flight mode. Hardware
compatible data received from the control module is converted to
a compatible format and sent on to JSBSim.

```
use gpio::Direction

struct Pin
{
  value: u8,
}

FUNCTION new(pin_number: u64) -> Option<gpio>
  pin <- INITIALIZE(pin_number)
  RETURN struct if did not fail
END FUNCTION

FUNCTION set_direction(dir: Direction)
  data <- INITIALIZE(dir)
  buffer_to_jsbsim(data)
END FUNCTION

FUNCTION set_value(value: u8) -> Option<()>
  SET pin_value TO value of 1 or 0
  data <- INITIALIZE(pin_value)
  buffer_to_jsbsim(data)
  RETURN okay if try did not fail
END FUNCTION

FUNCTION get_value() -> Option<u8>
  data = buffer_from_jsbsim()
  pin <- (data)
  IF pin is ESTOP RETURN 0
  RETURN pin.value
END FUNCTION

}

```

#### Sensor Interface
The sensor interface provides a set of functions that are equivalent to the
function calls made by the sensor module in flight mode. JSBSim sensor data
is retrieved, converted into a hardware compatible format, and made available
to the sensor module.

```
FUNCTION new()
  INITIALIZE gyro(gyro_ADXL345B)
  INITIALIZE accel(accel_L3G4200D)
  INITIALIZE JSBsim
  RETURN okay if try did not fail
END FUNCTION

FUNCTION read(address: u8) -> Option<u16>
  accel, gyro <- buffer_from_jsbsim()
  data <- Convert to MPU-6050 format {accel, gyro}
  return data
END FUNCTION

FUNCTION write(address: u8) -> Option<u16>
  data <- INITIALIZE
  buffer_to_jsbsim(data)
  RETURN okay if try did not fail
END FUNCTION

```

#### JSBSim
JSBSim is an open source C++ library.  It is used to simulate sensor outputs based on control inputs for flight simulations.

In order to work with JSBSim from Rust, it is necessary to create a wrapper for JSBSim.  This wrapper is implemented in three files:  wrapper.cpp, wrapper.h, & wrapper.rs.  The wrapper.cpp file contains the native C++ calls to JSBSim wrapped in C functions.  The wrapper.h file contains the externed C headers for these wrapped functions.  The wrapper.rs file contain the raw Rust calls to the C abi as well as a set of basic functions to allow a Rust-based program to run a JSBSim Flight Dynamics Model and run it via script.
```
File Name:          wrapper.rs
Purpose:            define a Rust-based front end for JSBSim
Components:
wrapper_init()     instantiate and initialize a JSBSim Flight Dynamics Model
send_to_jsbsim()   update FDM with data from the controller interface
get_from_jsbsim()  iterate the flight dynamics model by one step
                    provide property to allow scripts to end the simulation
                    update the sensor interface with gyro data from jsbsim
wrapper_close()    close the FDM and reset fdm to null (not implemented)
extern block       provide rust-based front end for the c abi defined in wrapper.h & wrapper.cpp


Function:           wrapper_init
Purpose:            instantiate and initialize a JSBSim Flight Dynamics Model
Inputs:             none
Outputs:            none
Side Effects:       fdm, a pointer with 'static lifetime is set to the instantiated FDM
Notes:              this function must be called previous to any other functions

FUNCTION wrapper_init
     INSTANTIATE a new Flight Dynamics Model
     SET base jsbsim scripts folder
     SET jsbsim directory structure (aircraft, engine, systems) & verify
     LOAD script & verify
     RUN initial conditions & verify
ENDFUNCTION


Function:           send_to_jsbsim
Inputs:             cw: u8, ccw: u8
Outputs:            none
Side Effects:       none
Purpose:            update FDM with data from the controller interface
Notes:              this function will indicate to the fdm that the flight controller is engaging
                    the controller interface i.e. firing thrusters

FUNCTION send_to_jsbsim
     SET the testmode/ledcw property in the JSBSim fdm to cw
     SET the testmode/ledccw property in the JSBSim fdm to ccw
ENDFUNCTION


Function:           get_from_jsbsim
Inputs:             none
Outputs:            gyro_x: f32, gyro_y: f32, gyro_z: f32
Side Effects:       none
Purpose:            iterate the flight dynamics model by one step
                    provide a property to allow scripts to end the simulation
                    update the sensor interface with gyro data from jsbsim

FUNCTION get_from_jsbsim
-RUN jsbsim exec's run method to iterate the flight dynamics model by one step
-GET sensor data from the fdm
-CHECK the endscript property & exit the application as indicated
-SEND the gyro data from JSBSim to the sensor interface

Function:           wrapper_close
Inputs:             none
Outputs:            none
Side Effects:       closes the fdm & sets the fdm pointer to null
Purpose:            provide a clean way to exit the simulation
Notes:              this function is not currently implemented

FUNCTION wrapper_close
     CLOSE the current fdm
     SET the fdm pointer to null
ENDFUNCTION


Component:          extern block
Purpose:            provide linkage to JSBSim as a shared object or dynamic link library
                    provide linkage to wrapper.h & wrapper.cpp as a static library
                    provide a basic set of functions to access JSBSim via the c abi defined
                         in wrapper.h & wrapper.cpp
Notes:              the functions in the extern block must parallel the c headers in wrapper.h
```
```
File Name:          wrapper.h
Purpose:            provide a set of c function definitions to wrap the c++ calls used to 
                         acess JSBSim
Notes:              the functions definitions listed here must parallel the function implementations
                         in wrapper.cpp
```
```
File Name:          wrapper.h
Purpose:            implement the c abi based wrapper functions defined in wrapper.h and that 
                         wrap teh c++ function calls to JSBSim
Notes:              the functions implementations listed here must parallel the function
                         definitions in wrapper.h
                    the function calls to JSBSim must match the function interface defined by 
                         JSBSim's FGFDMExec class
```
