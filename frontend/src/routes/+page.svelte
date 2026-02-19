<script lang="ts">
	import { onMount, onDestroy } from 'svelte';
	import DataTable from '$lib/components/commands-table/data-table.svelte';
	import ThemeToggle from '$lib/components/ThemeToggle.svelte';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import { AlertTriangle, RefreshCw, Loader2 } from 'lucide-svelte';
	import { toast } from 'svelte-sonner';
	import type { CommandEntry, RegistryError } from '$lib/components/commands-table/types';

	interface CommandListResponse {
		commands: CommandEntry[];
		total: number;
	}

	interface StatusResponse {
		last_updated: string | null;
		command_count: number;
		error_count: number;
		errors: RegistryError[];
		rebuilding: boolean;
	}

	let commands = $state<CommandEntry[]>([]);
	let errors = $state<RegistryError[]>([]);
	let lastUpdated = $state<string | null>(null);
	let rebuilding = $state(false);
	let reloadLoading = $state(false);

	let refreshInterval: ReturnType<typeof setInterval> | undefined;

	async function fetchCommands() {
		try {
			const res = await fetch('/api/commands');
			if (!res.ok) throw new Error('Failed to fetch commands');
			const data: CommandListResponse = await res.json();
			commands = data.commands;
		} catch (e) {
			console.error('Failed to fetch commands:', e);
		}
	}

	async function fetchStatus() {
		try {
			const res = await fetch('/api/status');
			if (!res.ok) throw new Error('Failed to fetch status');
			const data: StatusResponse = await res.json();
			lastUpdated = data.last_updated;
			errors = data.errors;
			rebuilding = data.rebuilding;
		} catch (e) {
			console.error('Failed to fetch status:', e);
		}
	}

	async function loadData() {
		await Promise.all([fetchCommands(), fetchStatus()]);
	}

	async function handleReload() {
		reloadLoading = true;
		try {
			const res = await fetch('/api/reload', { method: 'POST' });
			const data = await res.json();
			if (!res.ok) {
				throw new Error(data.message || 'Failed to reload');
			}
			toast.success(data.message || 'Reload triggered successfully');
			// Refresh data after a short delay to allow rebuild
			setTimeout(loadData, 1000);
		} catch (e) {
			toast.error(e instanceof Error ? e.message : 'Failed to reload');
		} finally {
			reloadLoading = false;
		}
	}

	function formatTimestamp(ts: string): string {
		try {
			return new Date(ts).toLocaleString();
		} catch {
			return ts;
		}
	}

	onMount(() => {
		loadData();
		refreshInterval = setInterval(loadData, 10000);
	});

	onDestroy(() => {
		if (refreshInterval) clearInterval(refreshInterval);
	});
</script>

<svelte:head>
	<title>iron-bunny</title>
</svelte:head>

<div class="min-h-screen bg-background p-6">
	<div class="max-w-5xl mx-auto space-y-6">
		<!-- Header -->
		<div class="flex justify-between items-start">
			<div>
				<h1 class="text-4xl font-bold tracking-tight">iron-bunny</h1>
				<p class="text-muted-foreground mt-1">Browser shortcut & redirect service</p>
			</div>
			<div class="flex items-center gap-4">
				<div class="text-right space-y-1">
					<div class="text-sm text-muted-foreground">
						{commands.length}
						{commands.length === 1 ? 'command' : 'commands'}
					</div>
					{#if lastUpdated}
						<div class="text-xs text-muted-foreground">
							Updated: {formatTimestamp(lastUpdated)}
						</div>
					{/if}
				</div>
				{#if rebuilding}
					<div class="flex items-center gap-2 text-primary">
						<Loader2 class="h-4 w-4 animate-spin" />
						<span class="text-sm font-medium">Rebuilding...</span>
					</div>
				{/if}
				<Button
					variant="outline"
					size="sm"
					onclick={handleReload}
					disabled={reloadLoading}
					class="gap-2"
				>
					{#if reloadLoading}
						<Loader2 class="h-4 w-4 animate-spin" />
					{:else}
						<RefreshCw class="h-4 w-4" />
					{/if}
					Reload
				</Button>
				<ThemeToggle />
			</div>
		</div>

		<!-- Error Alerts -->
		{#if errors.length > 0}
			<Alert.Root variant="destructive">
				<AlertTriangle class="h-4 w-4" />
				<Alert.Title>Errors Detected ({errors.length})</Alert.Title>
				<Alert.Description>
					<div class="mt-3 space-y-2">
						{#each errors as error, i (i)}
							<div
								class="text-sm border-l-2 border-destructive-foreground/50 pl-3 py-1"
							>
								<div class="font-semibold">
									{error.type.replace(/_/g, ' ')}
								</div>
								<div class="text-xs space-x-2 mt-1">
									{#if error.alias}
										<span>Command: {error.alias}</span>
									{/if}
									{#if error.container_name}
										<span>Container: {error.container_name}</span>
									{/if}
								</div>
								{#if error.error}
									<code
										class="block text-xs mt-1 opacity-90"
										style="font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;"
										>{error.error}</code
									>
								{/if}
								{#if error.details}
									<code
										class="block text-xs mt-1 opacity-90"
										style="font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;"
										>{error.details}</code
									>
								{/if}
							</div>
						{/each}
					</div>
				</Alert.Description>
			</Alert.Root>
		{/if}

		<!-- Commands Table -->
		<DataTable {commands} />
	</div>
</div>
