<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import * as Card from "$lib/components/ui/card";
	import * as Field from "$lib/components/ui/field";
	import { Input } from "$lib/components/ui/input";
	import { Label } from "$lib/components/ui/label";
	import { Switch } from "$lib/components/ui/switch";
	import { Separator } from "$lib/components/ui/separator";
	import { Button } from "$lib/components/ui/button";
	import MusicIcon from "@lucide/svelte/icons/music-2";
	import Disc3Icon from "@lucide/svelte/icons/disc-3";
	import HeadphonesIcon from "@lucide/svelte/icons/headphones";
	import MessageSquareIcon from "@lucide/svelte/icons/message-square";
	import EyeIcon from "@lucide/svelte/icons/eye";
	import EyeOffIcon from "@lucide/svelte/icons/eye-off";
	import SaveIcon from "@lucide/svelte/icons/save";
	import RotateCcwIcon from "@lucide/svelte/icons/rotate-ccw";
	import LastFMIcon from "$lib/components/icons/lastfm.svelte";
	import ListenBrainzIcon from "$lib/components/icons/listenbrainz.svelte";
	import LibreFMIcon from "$lib/components/icons/librefm.svelte";

	type ScrobbleService = {
		id: "lastfm" | "listenbrainz" | "librefm";
		name: string;
		description: string;
		keyLabel: string;
		keyPlaceholder: string;
		keyHelp: string;
	};

	const services: ScrobbleService[] = [
		{
			id: "lastfm",
			name: "Last.fm",
			description: "Scrobble tracks to your Last.fm profile.",
			keyLabel: "API key",
			keyPlaceholder: "Paste your Last.fm API key",
			keyHelp: "Generate one at <a href=\"https://last.fm/api/account/create\" target=\"_blank\">last.fm/api/account/create</a>.",
		},
		{
			id: "listenbrainz",
			name: "ListenBrainz",
			description: "Submit listens to ListenBrainz.",
			keyLabel: "User token",
			keyPlaceholder: "Paste your ListenBrainz user token",
			keyHelp: "Find it at <a href=\"https://listenbrainz.org/settings\" target=\"_blank\">listenbrainz.org/settings</a>.",
		},
		// {
		// 	id: "librefm",
		// 	name: "Libre.fm",
		// 	description: "Scrobble tracks to Libre.fm.",
		// 	keyLabel: "API key",
		// 	keyPlaceholder: "Paste your Libre.fm API key",
		// 	keyHelp: "Request one from the Libre.fm API page.",
		// },
	];

	type ScrobbleConfig = {
		enabled: boolean;
		apiKey: string;
	};

	type Config = Record<ScrobbleService["id"], ScrobbleConfig> & {
		discordRichPresence: boolean;
	};

	const defaults: Config = {
		lastfm: { enabled: false, apiKey: "" },
		listenbrainz: { enabled: false, apiKey: "" },
		librefm: { enabled: false, apiKey: "" },
		discordRichPresence: false,
	};

	let config = $state<Config>(structuredClone(defaults));
	let revealed = $state<Record<ScrobbleService["id"], boolean>>({
		lastfm: false,
		listenbrainz: false,
		librefm: false,
	});

	function reset() {
		config = structuredClone(defaults);
		revealed = { lastfm: false, listenbrainz: false, librefm: false };
	}

	async function save() {
		try {
			await invoke("save_config", { config });
		} catch (err) {
			console.warn("save_config unavailable:", err);
		}
	}

	function toggleReveal(id: ScrobbleService["id"]) {
		revealed[id] = !revealed[id];
	}
</script>

<main class="mx-auto flex w-full max-w-2xl flex-col gap-6 p-6">
	<header class="flex flex-col gap-1">
		<h1 class="text-2xl font-semibold tracking-tight">dotsong</h1>
		<p class="text-muted-foreground text-sm">
			Configure your scrobbling services and Discord integration.
		</p>
	</header>

	<section class="flex flex-col gap-3">
		<div class="flex items-center gap-2">
			<MusicIcon class="text-muted-foreground size-4" />
			<h2 class="text-sm font-medium">Scrobbling</h2>
		</div>

		{#each services as service (service.id)}
			<Card.Root>
				<Card.Header
					class="flex flex-row items-start justify-between gap-4 space-y-0"
				>
				<div class="flex flex-col gap-1">
					<div class="flex items-center gap-2">
						{#if service.id === "lastfm"}
							<LastFMIcon class="h-[1.2em]" />
						{:else if service.id === "listenbrainz"}
							<ListenBrainzIcon class="h-[1.2em]" />
						{:else}
							<LibreFMIcon class="h-[1.2em]" />
						{/if}
						<Card.Title>{service.name}</Card.Title>
					</div>
					<Card.Description
						>{service.description}</Card.Description
					>
				</div>
					<div class="flex items-center gap-2 pt-1">
						<Label
							for="{service.id}-enabled"
							class="text-muted-foreground text-sm"
						>
							{config[service.id].enabled
								? "Enabled"
								: "Disabled"}
						</Label>
						<Switch
							id="{service.id}-enabled"
							bind:checked={config[service.id].enabled}
						/>
					</div>
				</Card.Header>
				<Card.Content>
					<Separator class="mb-4" />
					<Field.Field>
						<Field.FieldLabel for="{service.id}-key"
							>{service.keyLabel}</Field.FieldLabel
						>
						<div class="flex gap-2">
							<Input
								id="{service.id}-key"
								type={revealed[service.id]
									? "text"
									: "password"}
								bind:value={config[service.id].apiKey}
								placeholder={service.keyPlaceholder}
								disabled={!config[service.id].enabled}
								autocomplete="off"
								spellcheck="false"
							/>
							<Button
								type="button"
								variant="outline"
								size="icon"
								disabled={!config[service.id].apiKey}
								onclick={() => toggleReveal(service.id)}
								aria-label={revealed[service.id]
									? "Hide {service.keyLabel}"
									: "Show {service.keyLabel}"}
							>
								{#if revealed[service.id]}
									<EyeOffIcon />
								{:else}
									<EyeIcon />
								{/if}
							</Button>
						</div>
						<Field.FieldDescription>
							{@html service.keyHelp} Stored locally on this machine.
						</Field.FieldDescription>
					</Field.Field>
				</Card.Content>
			</Card.Root>
		{/each}
	</section>

	<section class="flex flex-col gap-3">
		<div class="flex items-center gap-2">
			<MessageSquareIcon class="text-muted-foreground size-4" />
			<h2 class="text-sm font-medium">Integrations</h2>
		</div>

		<Card.Root>
			<Card.Header
				class="flex flex-row items-center justify-between gap-4 space-y-0"
			>
				<div class="flex flex-col gap-1">
					<Card.Title>Discord Rich Presence</Card.Title>
					<Card.Description>
						Show the currently playing track in your Discord status.
					</Card.Description>
				</div>
				<div class="flex items-center gap-2">
					<Label
						for="discord-enabled"
						class="text-muted-foreground text-sm"
					>
						{config.discordRichPresence ? "Enabled" : "Disabled"}
					</Label>
					<Switch
						id="discord-enabled"
						bind:checked={config.discordRichPresence}
					/>
				</div>
			</Card.Header>
		</Card.Root>
	</section>

	<footer class="flex items-center justify-end gap-2">
		<Button variant="ghost" onclick={reset}>
			<RotateCcwIcon data-icon="inline-start" />
			Reset
		</Button>
		<Button onclick={save}>
			<SaveIcon data-icon="inline-start" />
			Save changes
		</Button>
	</footer>
</main>
