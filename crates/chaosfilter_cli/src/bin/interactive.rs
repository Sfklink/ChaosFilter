//Entry point for the CLI

//main is minimal as it is only responsible for initializing the CLI
//and transfer control to the CLI runtime logic

//Keeping main small makes it easier to reason about startup actions
//and avoids joining high-level app flow with the user interaction

use chaosfilter_cli::cli;
use chaosfilter_cli::modules;

fn main() {
	//Starts the CLI loop
	//All user interaction and routing logic happens inside cli::run
	cli::run();
}
