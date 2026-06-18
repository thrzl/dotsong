<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import { Input } from "$lib/components/ui/input";
	import { Button } from "$lib/components/ui/button";
	import { Switch } from "$lib/components/ui/switch";
	import { Separator } from "$lib/components/ui/separator";
	import * as Field from "$lib/components/ui/field";
	import * as ToggleGroup from "$lib/components/ui/toggle-group";
	import * as InputGroup from "$lib/components/ui/input-group";
	import * as Empty from "$lib/components/ui/empty";
	import PlusIcon from "@lucide/svelte/icons/plus";
	import XIcon from "@lucide/svelte/icons/x";
	import EyeIcon from "@lucide/svelte/icons/eye";
	import EyeOffIcon from "@lucide/svelte/icons/eye-off";
	import SaveIcon from "@lucide/svelte/icons/save";
	import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
	import {onMount} from "svelte";
	import {getCurrentWindow} from "@tauri-apps/api/window";

	type ScrobblerFormat = "LastFM" | "ListenBrainz";

	type Scrobbler = {
		id: string;
		format: ScrobblerFormat;
		endpoint_url: string;
		api_key: string;
		revealed: boolean;
	};

	type Config = {
		scrobblers: Scrobbler[];
		discord_rpc_enabled: boolean;
	};

	const FORMATS = [
		{
			value: "LastFM" as const,
			label: "last.fm",
			keyLabel: "api key",
			defaultEndpoint: "https://ws.audioscrobbler.com/2.0/",
		},
		{
			value: "ListenBrainz" as const,
			label: "listenbrainz",
			keyLabel: "user token",
			defaultEndpoint: "https://api.listenbrainz.org/1/",
		},
	];

	const formatMeta = (f: ScrobblerFormat) =>
		FORMATS.find((x) => x.value === f)!;

	function newScrobbler(format: ScrobblerFormat = "LastFM"): Scrobbler {
		return {
			id: crypto.randomUUID(),
			format,
			endpoint_url: formatMeta(format).defaultEndpoint,
			api_key: "",
			revealed: false,
		};
	}

	const defaults: Config = {
		scrobblers: [],
		discord_rpc_enabled: false,
	};

	let config = $state<Config>(structuredClone(defaults));

	function reset() {
		config = structuredClone(defaults);
	}

	function addScrobbler(format: ScrobblerFormat) {
		config.scrobblers = [...config.scrobblers, newScrobbler(format)];
	}

	function removeScrobbler(id: string) {
		config.scrobblers = config.scrobblers.filter((s) => s.id !== id);
	}

	function changeFormat(s: Scrobbler, newFormat: string | null) {
		if (!newFormat) return;
		const format = newFormat as ScrobblerFormat;
		const isDefault = FORMATS.some((f) => f.defaultEndpoint === s.endpoint_url);
		s.format = format;
		if (isDefault) s.endpoint_url = formatMeta(format).defaultEndpoint;
	}

	async function save() {
		console.log("saving config...");
		try {
			const payload = {
				scrobblers: config.scrobblers.map((s) => ({
					id: s.id,
					endpoint_url: s.endpoint_url,
					api_key: s.api_key,
					format: s.format,
				})),
				discord_rpc_enabled: config.discord_rpc_enabled,
			};
			await invoke("save_config", { config: payload });
		} catch (err) {
			console.error("save_config unavailable:", err);
		}
		await close();
	}

	async function close() {
		const window = getCurrentWindow();
		await window.close();
	}

	onMount(async () => {
		console.log("loading config...");
		try {
			const savedConfig = await invoke<Config>("load_config");
			config = savedConfig;
			console.log("saved:", config)
		} catch (err) {
			console.error("load_config unavailable:", err);
		}
	});
</script>

<main class="mx-auto flex w-full max-w-2xl flex-col gap-6 px-6 py-8 text-sm">
	<header class="flex flex-col gap-1">
		<h1 class="text-foreground text-lg font-semibold tracking-tight lowercase">
			dotsong settings
		</h1>
	</header>

	<Separator />

	<Field.Field orientation="horizontal">
		<Field.FieldContent>
			<Field.FieldTitle>discord rich presence</Field.FieldTitle>
			<Field.FieldDescription>
				show the now-playing track in your discord status
			</Field.FieldDescription>
		</Field.FieldContent>
		<Switch
			id="discord-rpc"
			bind:checked={config.discord_rpc_enabled}
			aria-label="discord rich presence"
		/>
	</Field.Field>

	<Separator />

	<section class="flex flex-col gap-4">
		<div class="flex items-baseline justify-between">
			<h2 class="text-foreground text-sm font-semibold">scrobbling targets</h2>
			{#if config.scrobblers.length > 0}
				<span class="text-muted-foreground text-xs tabular-nums">
					{config.scrobblers.length} configured
				</span>
			{/if}
		</div>

		{#if config.scrobblers.length > 0}
			<ul class="border-border divide-border divide-y border-y">
				{#each config.scrobblers as s (s.id)}
					<li class="grid grid-cols-[1fr_auto] items-start gap-x-3 py-3">
						<div class="flex min-w-0 flex-col gap-2">
							<div class="flex items-center gap-2">
								<ToggleGroup.Root
									type="single"
									value={s.format}
									onValueChange={(v) => changeFormat(s, v)}
									variant="outline"
									size="sm"
									aria-label="format"
									class="shrink-0"
								>
									<ToggleGroup.Item value="LastFM">last.fm</ToggleGroup.Item>
									<ToggleGroup.Item value="ListenBrainz">listenbrainz</ToggleGroup.Item>
								</ToggleGroup.Root>
								<Input
									bind:value={s.endpoint_url}
									aria-label="endpoint url"
									class="h-7 min-w-0 flex-1 rounded-md font-mono text-xs"
									placeholder="https://..."
									spellcheck="false"
									autocomplete="off"
								/>
							</div>
							<InputGroup.Root class="h-7 rounded-md">
								<InputGroup.Addon align="inline-start" class="px-2">
									<InputGroup.Text class="text-xs">
										{formatMeta(s.format).keyLabel}
									</InputGroup.Text>
								</InputGroup.Addon>
								<InputGroup.Input
									type={s.revealed ? "text" : "password"}
									bind:value={s.api_key}
									placeholder="paste here"
									spellcheck="false"
									autocomplete="off"
									class="font-mono text-xs"
								/>
								<InputGroup.Addon align="inline-end" class="px-1">
									<InputGroup.Button
										size="icon-xs"
										onclick={() => (s.revealed = !s.revealed)}
										disabled={!s.api_key}
										aria-label={s.revealed ? "hide key" : "show key"}
									>
										{#if s.revealed}
											<EyeOffIcon />
										{:else}
											<EyeIcon />
										{/if}
									</InputGroup.Button>
								</InputGroup.Addon>
							</InputGroup.Root>
						</div>
						<Button
							type="button"
							variant="ghost"
							size="icon"
							class="text-muted-foreground hover:text-foreground hover:bg-destructive/10 mt-1.5 size-7 shrink-0 rounded-md"
							onclick={() => removeScrobbler(s.id)}
							aria-label="remove target"
						>
							<XIcon />
						</Button>
					</li>
				{/each}
			</ul>
		{:else}
			<Empty.Root class="rounded-md border">
				<Empty.Header>
					<Empty.Title>no targets yet</Empty.Title>
					<Empty.Description>
						add a last.fm or listenbrainz target below to start scrobbling.
					</Empty.Description>
				</Empty.Header>
			</Empty.Root>
		{/if}

		<div class="flex flex-wrap items-center gap-2">
			<span class="text-muted-foreground text-xs">add:</span>
			{#each FORMATS as f (f.value)}
				<Button
					type="button"
					variant="outline"
					size="sm"
					class="rounded-md text-xs"
					onclick={() => addScrobbler(f.value)}
				>
					<PlusIcon data-icon="inline-start" />
					{f.label}
				</Button>
			{/each}
			<span class="text-muted-foreground ml-auto text-xs italic">
				pre-fills the default endpoint url
			</span>
		</div>
	</section>

	<Separator />

	<footer class="flex items-center justify-end gap-2">
		<Button variant="ghost" class="rounded-md text-xs" onclick={reset}>
			<RotateCcwIcon data-icon="inline-start" />
			reset
		</Button>
		<Button variant="secondary" class="rounded-md text-xs" onclick={close}>
			<!-- <SaveIcon data-icon="inline-start" /> -->
			cancel
		</Button>
		<Button class="rounded-md text-xs w-12" onclick={save}>
			<!-- <SaveIcon data-icon="inline-start" /> -->
			ok
		</Button>
	</footer>
</main>
