<script lang="ts">
	import * as Table from '$lib/components/ui/table';
	import { Input } from '$lib/components/ui/input';
	import { Badge } from '$lib/components/ui/badge';
	import { ArrowUp, ArrowDown, Search } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';
	import type { CommandEntry, DisplayRow } from './types';

	let { commands = [] }: { commands: CommandEntry[] } = $props();

	let filterValue = $state('');
	let sortDirection = $state<'asc' | 'desc'>('asc');

	/** Flatten commands into display rows, expanding nested children */
	function flattenCommands(cmds: CommandEntry[]): DisplayRow[] {
		const rows: DisplayRow[] = [];
		for (const cmd of cmds) {
			// Parent row
			rows.push({
				parent_alias: cmd.alias,
				child_alias: '',
				display_alias: cmd.alias,
				description: cmd.description,
				is_error: cmd.is_error,
				error_message: cmd.error_message,
				is_nested_child: false
			});
			// Nested child rows
			if (cmd.has_nested && cmd.nested_commands) {
				const sorted = [...cmd.nested_commands].sort((a, b) =>
					a.alias.localeCompare(b.alias)
				);
				for (const child of sorted) {
					rows.push({
						parent_alias: cmd.alias,
						child_alias: child.alias,
						display_alias: `${cmd.alias} ${child.alias}`,
						description: child.description,
						is_error: false,
						error_message: null,
						is_nested_child: true
					});
				}
			}
		}
		return rows;
	}

	const displayRows = $derived(() => {
		const rows = flattenCommands(commands);
		// Filter
		const filtered = filterValue
			? rows.filter((row) => {
					const search = filterValue.toLowerCase();
					return (
						row.display_alias.toLowerCase().includes(search) ||
						row.description.toLowerCase().includes(search)
					);
				})
			: rows;
		// Sort by display_alias
		return [...filtered].sort((a, b) => {
			const aVal = a.display_alias;
			const bVal = b.display_alias;
			if (aVal < bVal) return sortDirection === 'asc' ? -1 : 1;
			if (aVal > bVal) return sortDirection === 'asc' ? 1 : -1;
			return 0;
		});
	});

	/** Total row count (parent + children) for the unfiltered list */
	const totalRowCount = $derived(() => flattenCommands(commands).length);

	function toggleSort() {
		sortDirection = sortDirection === 'asc' ? 'desc' : 'asc';
	}
</script>

<div class="space-y-4">
	<!-- Filter/Search Input -->
	<div class="flex items-center gap-2">
		<div class="relative flex-1 max-w-sm">
			<Search class="absolute left-2 top-1/2 h-4 w-4 -translate-y-1/2 text-muted-foreground" />
			<Input
				placeholder="Filter commands..."
				type="text"
				bind:value={filterValue}
				class="pl-8"
			/>
		</div>
		<div class="text-sm text-muted-foreground">
			{displayRows().length} of {totalRowCount()}
			{totalRowCount() === 1 ? 'command' : 'commands'}
		</div>
	</div>

	<!-- Table -->
	<div class="rounded-md border border-border">
		<Table.Root>
			<Table.Header>
				<Table.Row>
					<Table.Head class="font-medium">
						<Button variant="ghost" onclick={toggleSort} class="-ml-3 h-8">
							Command
							{#if sortDirection === 'asc'}
								<ArrowUp class="ml-2 h-4 w-4" />
							{:else}
								<ArrowDown class="ml-2 h-4 w-4" />
							{/if}
						</Button>
					</Table.Head>
					<Table.Head class="font-medium">Description</Table.Head>
				</Table.Row>
			</Table.Header>
			<Table.Body>
				{#if displayRows().length > 0}
					{#each displayRows() as row, i (row.display_alias + ':' + i)}
						<Table.Row
							class={row.is_error
								? 'bg-destructive/10 hover:bg-destructive/20'
								: ''}
						>
							<Table.Cell>
								<div class="flex items-center gap-2">
									{#if row.is_nested_child}
										<code
											class="text-sm font-mono text-muted-foreground"
											style="font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;"
										>
											{row.parent_alias}
										</code>
										<code
											class="text-sm font-mono"
											style="font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;"
										>
											{row.child_alias}
										</code>
									{:else}
										<code
											class="text-sm font-mono"
											style="font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;"
										>
											{row.display_alias}
										</code>
									{/if}
									{#if row.is_error}
										<Badge variant="destructive">error</Badge>
									{/if}
								</div>
							</Table.Cell>
							<Table.Cell>
								{#if row.is_error && row.error_message}
									<span class="text-destructive text-sm">{row.error_message}</span>
								{:else}
									<span class="text-sm">{row.description}</span>
								{/if}
							</Table.Cell>
						</Table.Row>
					{/each}
				{:else}
					<Table.Row>
						<Table.Cell colspan={2} class="h-24 text-center">
							<div
								class="flex flex-col items-center justify-center gap-2 text-muted-foreground"
							>
								<p>No commands found.</p>
								{#if filterValue}
									<p class="text-sm">Try adjusting your search filter.</p>
								{:else}
									<p class="text-sm">
										Add commands to <code
											class="px-1 py-0.5 rounded bg-muted text-xs"
											style="font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;"
											>commands.yaml</code
										> or add Docker containers with
										<code
											class="px-1 py-0.5 rounded bg-muted text-xs"
											style="font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;"
											>bunny.commands.*</code
										> labels.
									</p>
								{/if}
							</div>
						</Table.Cell>
					</Table.Row>
				{/if}
			</Table.Body>
		</Table.Root>
	</div>
</div>
