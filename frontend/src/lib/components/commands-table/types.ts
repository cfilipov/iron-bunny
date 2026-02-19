export interface CommandEntry {
	alias: string;
	url: string;
	description: string;
	source: 'yaml' | 'docker_label' | 'both';
	container_name: string | null;
	has_nested: boolean;
	is_error: boolean;
	error_message: string | null;
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
