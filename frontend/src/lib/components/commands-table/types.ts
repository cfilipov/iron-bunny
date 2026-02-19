export interface NestedCommandEntry {
	alias: string;
	url: string;
	description: string;
}

export interface CommandEntry {
	alias: string;
	url: string;
	description: string;
	source: 'yaml' | 'docker_label' | 'both';
	container_name: string | null;
	has_nested: boolean;
	nested_commands: NestedCommandEntry[];
	is_error: boolean;
	error_message: string | null;
}

/** Flattened row for display: either a parent command or a nested child */
export interface DisplayRow {
	/** The parent alias */
	parent_alias: string;
	/** The child alias (empty for parent rows) */
	child_alias: string;
	/** Combined display alias: "parent" or "parent child" */
	display_alias: string;
	description: string;
	is_error: boolean;
	error_message: string | null;
	is_nested_child: boolean;
}

export interface RegistryError {
	type: string;
	alias?: string;
	source?: string;
	details?: string;
	container_name?: string;
	error?: string;
	label?: string;
	winner?: string;
	timestamp: string;
}
