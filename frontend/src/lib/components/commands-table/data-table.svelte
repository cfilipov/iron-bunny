<script lang="ts">
	import * as Table from '$lib/components/ui/table';
	import { Input } from '$lib/components/ui/input';
	import { Badge } from '$lib/components/ui/badge';
	import { ArrowUp, ArrowDown, ArrowUpDown, Search } from 'lucide-svelte';
	import { Button } from '$lib/components/ui/button';
	import type { CommandEntry } from './types';

	let { commands = [] }: { commands: CommandEntry[] } = $props();

	let filterValue = $state('');
	let sortDirection = $state<'asc' | 'desc'>('asc');

	const filteredCommands = $derived(() => {
		if (!filterValue) return commands;
		const search = filterValue.toLowerCase();
		return commands.filter((cmd) => {
			return (
				cmd.alias.toLowerCase().includes(search) ||
				cmd.description.toLowerCase().includes(search)
			);
		});
	});

	const sortedCommands = $derived(() => {
		const filtered = filteredCommands();
		return [...filtered].sort((a, b) => {
			const aVal = a.alias;
			const bVal = b.alias;
			if (aVal < bVal) return sortDirection === 'asc' ? -1 : 1;
			if (aVal > bVal) return sortDirection === 'asc' ? 1 : -1;
			return 0;
		});
	});

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
			{sortedCommands().length} of {commands.length}
			{commands.length === 1 ? 'command' : 'commands'}
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
				{#if sortedCommands().length > 0}
					{#each sortedCommands() as cmd, i (cmd.alias + ':' + i)}
						<Table.Row
							class={cmd.is_error
								? 'bg-destructive/10 hover:bg-destructive/20'
								: ''}
						>
							<Table.Cell>
								<div class="flex items-center gap-2">
									<code
										class="text-sm font-mono"
										style="font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;"
									>
										{cmd.alias}
									</code>
									{#if cmd.is_error}
										<Badge variant="destructive">error</Badge>
									{/if}
									{#if cmd.has_nested}
										<Badge variant="secondary">nested</Badge>
									{/if}
								</div>
							</Table.Cell>
							<Table.Cell>
								{#if cmd.is_error && cmd.error_message}
									<span class="text-destructive text-sm">{cmd.error_message}</span>
								{:else}
									<span class="text-sm">{cmd.description}</span>
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
