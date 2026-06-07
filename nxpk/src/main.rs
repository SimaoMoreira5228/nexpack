use clap::{Parser, Subcommand};

mod commands;

#[derive(Parser)]
#[command(name = "nxpk", about = "Nexpack portable app manager")]
struct Cli {
	#[command(subcommand)]
	command: Commands,
}

#[derive(Subcommand)]
enum Commands {
	Run {
		bundle: String,

		#[arg(long)]
		sandbox: bool,

		#[arg(long)]
		no_sandbox: bool,

		#[arg(long)]
		offline: bool,

		#[arg(trailing_var_arg = true, allow_hyphen_values = true)]
		args: Vec<String>,
	},

	Install {
		bundle: String,
	},

	Update {
		app_id: Option<String>,

		#[arg(long)]
		all: bool,
	},

	Remove {
		app_id: String,
	},

	Gc,

	Inspect {
		bundle: String,

		#[arg(long)]
		json: bool,
	},

	Verify {
		bundle: String,

		#[arg(long)]
		offline: bool,
	},

	Permissions {
		app_id: String,

		#[arg(long)]
		edit: bool,
	},

	Trust {
		app_id_pattern: Option<String>,
		identity: Option<String>,

		#[arg(long)]
		edit: bool,
	},

	Pack {
		spec: String,
	},

	Export {
		app_id: String,
	},

	Sign {
		bundle: String,

		#[arg(long, short)]
		output: Option<String>,
	},

	#[command(subcommand)]
	Compat(CompatCommands),
}

#[derive(Subcommand)]
enum CompatCommands {
	Run {
		appimage: String,

		#[arg(trailing_var_arg = true, allow_hyphen_values = true)]
		args: Vec<String>,
	},
	Convert {
		appimage: String,
		output: Option<String>,
	},
}

fn main() -> anyhow::Result<()> {
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "nxpk=info".into()))
		.init();

	let cli = Cli::parse();

	match cli.command {
		Commands::Run {
			bundle,
			sandbox,
			no_sandbox,
			offline,
			args,
		} => {
			let sandbox_mode = if sandbox {
				Some(true)
			} else if no_sandbox {
				Some(false)
			} else {
				None
			};
			commands::run::run(&bundle, &args, sandbox_mode, offline)
		}
		Commands::Install { bundle } => commands::install::install(&bundle),
		Commands::Update { app_id, all } => commands::update::update(app_id.as_deref(), all),
		Commands::Remove { app_id } => commands::remove::remove(&app_id),
		Commands::Gc => commands::gc::gc(),
		Commands::Inspect { bundle, json } => commands::inspect::inspect(&bundle, json),
		Commands::Verify { bundle, offline } => commands::verify::verify(&bundle, offline),
		Commands::Permissions { app_id, edit } => commands::permissions::permissions(&app_id, edit),
		Commands::Trust {
			app_id_pattern,
			identity,
			edit,
		} => {
			if edit {
				commands::trust::edit_trust()
			} else if let (Some(pattern), Some(id)) = (app_id_pattern, identity) {
				commands::trust::trust(&pattern, &id)
			} else {
				anyhow::bail!("usage: nxpk trust <app_id> <identity>  or  nxpk trust --edit");
			}
		}
		Commands::Pack { spec } => commands::pack::pack(&spec),
		Commands::Export { app_id } => commands::export::export_app(&app_id),
		Commands::Sign { bundle, output } => commands::sign::sign_bundle(&bundle, output.as_deref()),
		Commands::Compat(cmd) => match cmd {
			CompatCommands::Run { appimage, args } => commands::compat::run_compat(&appimage, &args),
			CompatCommands::Convert { appimage, output } => commands::compat::convert_appimage(&appimage, output.as_deref()),
		},
	}
}
