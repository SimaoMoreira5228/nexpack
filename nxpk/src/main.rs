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
	},

	Permissions {
		app_id: String,

		#[arg(long)]
		edit: bool,
	},

	Trust {
		app_id_pattern: String,
		identity: String,
	},

	Pack {
		spec: String,
	},

	Export {
		app_id: String,
	},

	Search {
		query: String,
	},
}

fn main() -> anyhow::Result<()> {
	tracing_subscriber::fmt()
		.with_env_filter(tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "nxpk=info".into()))
		.init();

	let cli = Cli::parse();

	match cli.command {
		Commands::Run { bundle, args } => commands::run::run(&bundle, &args),
		Commands::Install { bundle } => commands::install::install(&bundle),
		Commands::Update { app_id, all } => commands::update::update(app_id.as_deref(), all),
		Commands::Remove { app_id } => commands::remove::remove(&app_id),
		Commands::Gc => commands::gc::gc(),
		Commands::Inspect { bundle, json } => commands::inspect::inspect(&bundle, json),
		Commands::Verify { bundle } => commands::verify::verify(&bundle),
		Commands::Permissions { app_id, edit } => commands::permissions::permissions(&app_id, edit),
		Commands::Trust {
			app_id_pattern,
			identity,
		} => commands::trust::trust(&app_id_pattern, &identity),
		Commands::Pack { spec } => commands::pack::pack(&spec),
		Commands::Export { app_id } => commands::export::export_app(&app_id),
		Commands::Search { query } => commands::search::search(&query),
	}
}
