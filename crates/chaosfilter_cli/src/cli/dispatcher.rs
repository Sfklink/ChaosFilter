//Routes user to the correct module based on their selection

//dispatcher is the centralized dispatcher that menu options to
//module entry points

//Centralizing this logic makes adding, removimg, or reordering
//modules easy and prevents modules from touching the menu or other module code

use crate::modules;

//Dispatches execution based on the users selection on the main menu

//Returns true to keep going and false to end

pub fn dispatch(choice: u32) -> bool {
	match choice {
		1 => {
			println!("Loading Network Stack module...");
			modules::network::run();
		}
		2 => {
			println!("Loading Disk I/O module...");
			modules::disk::run();
                }
		3 => {
			println!("Loading CPU module...");
			modules::cpu::run();
                }
		4 => return false,
		_ => println!("Invalid selection"),
	}

	true
}
