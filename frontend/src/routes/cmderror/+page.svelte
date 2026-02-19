<script lang="ts">
	import { page } from '$app/stores';
	import * as Alert from '$lib/components/ui/alert';
	import { Button } from '$lib/components/ui/button';
	import { AlertTriangle, ArrowLeft } from 'lucide-svelte';
	import { base } from '$app/paths';

	const alias = $derived($page.url.searchParams.get('alias') ?? 'unknown');
	const reason = $derived($page.url.searchParams.get('reason') ?? 'unknown');

	const reasonMessages: Record<string, string> = {
		duplicate:
			'This command has conflicting configurations from multiple sources. Please check your Docker labels and config file to ensure this command is defined only once.',
		missing_url:
			'This command is missing a required URL. Please check the Docker label configuration.',
		invalid_alias:
			'This command has an invalid alias. Aliases must be alphanumeric with hyphens or underscores, max 64 characters.',
		interpolation_error:
			'This command has a template interpolation error. A referenced label could not be found on the container.'
	};
</script>

<svelte:head>
	<title>Command Error - iron-bunny</title>
</svelte:head>

<div class="min-h-screen bg-background p-6">
	<div class="max-w-2xl mx-auto space-y-6">
		<div>
			<h1 class="text-4xl font-bold tracking-tight">iron-bunny</h1>
			<p class="text-muted-foreground mt-1">Command Configuration Error</p>
		</div>

		<Alert.Root variant="destructive">
			<AlertTriangle class="h-4 w-4" />
			<Alert.Title>
				Command
				<code
					class="px-1 py-0.5 rounded bg-destructive/20 text-sm"
					style="font-family: ui-monospace, SFMono-Regular, 'SF Mono', Menlo, Consolas, 'Liberation Mono', monospace;"
					>{alias}</code
				> is misconfigured
			</Alert.Title>
			<Alert.Description>
				<p class="mt-2">
					{reasonMessages[reason] ??
						`An unknown error occurred with this command (reason: ${reason}).`}
				</p>
			</Alert.Description>
		</Alert.Root>

		<Button variant="outline" href="{base}/" class="gap-2">
			<ArrowLeft class="h-4 w-4" />
			Back to dashboard
		</Button>
	</div>
</div>
