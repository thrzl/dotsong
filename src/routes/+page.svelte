<script lang="ts">
	import { invoke } from "@tauri-apps/api/core";
	import { Input } from "$lib/components/ui/input";
	import { Button } from "$lib/components/ui/button";
	import { Switch } from "$lib/components/ui/switch";
	import { Separator } from "$lib/components/ui/separator";
	import { Badge } from "$lib/components/ui/badge";
	import * as Field from "$lib/components/ui/field";
	import * as Dialog from "$lib/components/ui/dialog";
	import * as ToggleGroup from "$lib/components/ui/toggle-group";
	import * as InputGroup from "$lib/components/ui/input-group";
	import * as Empty from "$lib/components/ui/empty";
	import PlusIcon from "@lucide/svelte/icons/plus";
	import XIcon from "@lucide/svelte/icons/x";
	import EyeIcon from "@lucide/svelte/icons/eye";
	import EyeOffIcon from "@lucide/svelte/icons/eye-off";
	import PencilIcon from "@lucide/svelte/icons/pencil";
	import LogInIcon from "@lucide/svelte/icons/log-in";
	import { onMount } from "svelte";
	import { getCurrentWindow } from "@tauri-apps/api/window";

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
			id: "",
			format,
			endpoint_url: formatMeta(format).defaultEndpoint,
			api_key: "",
			revealed: false,
		};
	}

	function detectNameFromUrl(url: string): string {
		try {
			const host = new URL(url).hostname;
			if (host.endsWith(".audioscrobbler.com")) return "last.fm";
			if (host.endsWith(".listenbrainz.org")) return "listenbrainz";
			if (host.endsWith("libre.fm")) return "libre.fm";
			const parts = host.split(".");
			if (parts.length < 2) return host;
			return parts[parts.length - 2];
		} catch {
			return "";
		}
	}

	const defaults: Config = {
		scrobblers: [],
		discord_rpc_enabled: false,
	};

	let config = $state<Config>(structuredClone(defaults));

	let dialogOpen = $state(false);
	let editingId = $state<string | null>(null);
	let draft = $state<Scrobbler>(newScrobbler());

	$effect(() => {
		if (!dialogOpen) draft.revealed = false;
	});

	$effect(() => {
		const detected = detectNameFromUrl(draft.endpoint_url);
		if (!draft.id && detected) draft.id = detected;
	});

	const nameError = $derived.by(() => {
		const name = draft.id.trim();
		if (!name) return "name is required";
		if (/\s/.test(name)) return "name cannot contain spaces";
		const duplicate = config.scrobblers.some(
			(s) => s.id === name && s.id !== editingId,
		);
		if (duplicate) return "another target already uses this name";
		return null;
	});

	function isOfficialLastFm(url: string): boolean {
		try {
			return new URL(url).hostname.endsWith(".audioscrobbler.com");
		} catch {
			return false;
		}
	}

	const showLastFmLogin = $derived(
		draft.format === "LastFM" && isOfficialLastFm(draft.endpoint_url),
	);

	function loginWithLastFm() {
		// TODO: open last.fm auth flow
		console.log("login with last.fm (not yet implemented)");
	}

	function reset() {
		config = structuredClone(defaults);
	}

	function removeScrobbler(id: string) {
		config.scrobblers = config.scrobblers.filter((s) => s.id !== id);
	}

	function changeFormat(s: Scrobbler, newFormat: string | null) {
		if (!newFormat) return;
		const format = newFormat as ScrobblerFormat;
		const isDefault = FORMATS.some(
			(f) => f.defaultEndpoint === s.endpoint_url,
		);
		s.format = format;
		if (isDefault) s.endpoint_url = formatMeta(format).defaultEndpoint;
	}

	function openAdd() {
		editingId = null;
		draft = newScrobbler();
		dialogOpen = true;
	}

	function openEdit(s: Scrobbler) {
		editingId = s.id;
		draft = structuredClone($state.snapshot(s));
		dialogOpen = true;
	}

	function commitDraft() {
		if (editingId === null) {
			config.scrobblers = [...config.scrobblers, draft];
		} else {
			config.scrobblers = config.scrobblers.map((s) =>
				s.id === editingId ? draft : s,
			);
		}
		dialogOpen = false;
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
			console.log("saved:", config);
		} catch (err) {
			console.error("load_config unavailable:", err);
		}
	});
</script>

<main class="mx-auto flex w-full max-w-2xl flex-col gap-6 px-6 py-8 text-sm">
	<header class="flex flex-col gap-1">
		<h1
			class="text-foreground text-lg font-semibold tracking-tight lowercase"
		>
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
			<h2 class="text-foreground text-sm font-semibold">
				scrobbling targets
			</h2>
			<div class="flex items-center gap-3">
				{#if config.scrobblers.length > 0}
					<span class="text-muted-foreground text-xs tabular-nums">
						{config.scrobblers.length} configured
					</span>
				{/if}
				<Button
					type="button"
					variant="outline"
					size="sm"
					class="rounded-md text-xs"
					onclick={openAdd}
				>
					<PlusIcon data-icon="inline-start" />
					add
				</Button>
			</div>
		</div>

		{#if config.scrobblers.length > 0}
			<ul class="border-border divide-border divide-y border-y">
				{#each config.scrobblers as s (s.id)}
					<li class="flex items-start justify-between gap-3 py-2.5">
						<div class="flex min-w-0 flex-1 flex-col gap-1.5">
							<div
								class="flex flex-wrap items-center gap-x-2 gap-y-1"
							>
								{#if s.id}
									<span
										class="text-foreground truncate text-sm font-medium"
										title={s.id}
									>
										{s.id}
									</span>
								{:else}
									<span
										class="text-muted-foreground text-sm italic"
										>unnamed</span
									>
								{/if}
								<Badge
									variant="secondary"
									class="shrink-0 text-xs"
								>
									{formatMeta(s.format).label}
								</Badge>
								{#if s.api_key}
									<Badge
										variant="outline"
										class="shrink-0 text-xs font-normal text-emerald-600 dark:text-emerald-400"
									>
										key set
									</Badge>
								{:else}
									<Badge
										variant="outline"
										class="text-muted-foreground shrink-0 text-xs font-normal italic"
									>
										no key
									</Badge>
								{/if}
							</div>
							<span
								class="text-muted-foreground truncate font-mono text-xs"
								title={s.endpoint_url}
							>
								{s.endpoint_url}
							</span>
						</div>
						<div class="flex shrink-0 items-center gap-1">
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								class="text-muted-foreground hover:text-foreground rounded-md"
								onclick={() => openEdit(s)}
								aria-label="edit target"
							>
								<PencilIcon />
							</Button>
							<Button
								type="button"
								variant="ghost"
								size="icon-sm"
								class="text-muted-foreground hover:text-foreground hover:bg-destructive/10 rounded-md"
								onclick={() => removeScrobbler(s.id)}
								aria-label="remove target"
							>
								<XIcon />
							</Button>
						</div>
					</li>
				{/each}
			</ul>
		{:else}
			<Empty.Root class="rounded-md border">
				<Empty.Header>
					<Empty.Title>no targets yet</Empty.Title>
					<Empty.Description>
						add a last.fm or listenbrainz target to start
						scrobbling.
					</Empty.Description>
				</Empty.Header>
			</Empty.Root>
		{/if}
	</section>

	<Separator />

	<footer class="flex items-center justify-end gap-2">
		<Button variant="ghost" class="rounded-md text-xs" onclick={reset}>
			reset
		</Button>
		<Button variant="secondary" class="rounded-md text-xs" onclick={close}>
			cancel
		</Button>
		<Button class="rounded-md text-xs w-12" onclick={save}>ok</Button>
	</footer>
</main>

<Dialog.Root bind:open={dialogOpen}>
	<Dialog.Content>
		<Dialog.Header>
			<Dialog.Title
				>{editingId === null
					? "add scrobbler"
					: "edit scrobbler"}</Dialog.Title
			>
			<Dialog.Description>
				configure a scrobbling target. the format determines which api
				to talk to.
			</Dialog.Description>
		</Dialog.Header>
		<Field.FieldGroup>
			<Field.Field data-invalid={nameError !== null}>
				<Field.FieldLabel for="scrobbler-name">name</Field.FieldLabel>
				<Input
					id="scrobbler-name"
					bind:value={draft.id}
					placeholder="e.g. personal, work"
					spellcheck="false"
					autocomplete="off"
					class="text-xs"
					aria-invalid={nameError !== null}
				/>
				<Field.FieldDescription>
					a unique identifier for this target. no spaces.
				</Field.FieldDescription>
				{#if nameError}
					<Field.FieldError>{nameError}</Field.FieldError>
				{/if}
			</Field.Field>
			<Field.Field>
				<Field.FieldLabel>format</Field.FieldLabel>
				<ToggleGroup.Root
					type="single"
					value={draft.format}
					onValueChange={(v) => changeFormat(draft, v)}
					variant="outline"
					size="sm"
					aria-label="format"
				>
					<ToggleGroup.Item value="LastFM">last.fm</ToggleGroup.Item>
					<ToggleGroup.Item value="ListenBrainz"
						>listenbrainz</ToggleGroup.Item
					>
				</ToggleGroup.Root>
			</Field.Field>
			<Field.Field>
				<Field.FieldLabel for="endpoint-url"
					>endpoint url</Field.FieldLabel
				>
				<Input
					id="endpoint-url"
					bind:value={draft.endpoint_url}
					placeholder="https://..."
					spellcheck="false"
					autocomplete="off"
					class="font-mono text-xs"
				/>
			</Field.Field>
			{#if showLastFmLogin}
				<Field.Field>
					<Button
						type="button"
						variant="default"
						class="w-full"
						onclick={loginWithLastFm}
					>
						<LogInIcon data-icon="inline-start" />
						login with last.fm
					</Button>
					<Field.FieldDescription>
						authenticate with last.fm to enable scrobbling
					</Field.FieldDescription>
				</Field.Field>
			{:else}
				<Field.Field>
					<Field.FieldLabel for="api-key">
						{formatMeta(draft.format).keyLabel}
					</Field.FieldLabel>
					<InputGroup.Root>
						<InputGroup.Input
							id="api-key"
							type={draft.revealed ? "text" : "password"}
							bind:value={draft.api_key}
							placeholder="paste here"
							spellcheck="false"
							autocomplete="off"
							class="font-mono text-xs"
						/>
						<InputGroup.Addon align="inline-end" class="px-1">
							<InputGroup.Button
								size="icon-xs"
								onclick={() => (draft.revealed = !draft.revealed)}
								disabled={!draft.api_key}
								aria-label={draft.revealed
									? "hide key"
									: "show key"}
							>
								{#if draft.revealed}
									<EyeOffIcon />
								{:else}
									<EyeIcon />
								{/if}
							</InputGroup.Button>
						</InputGroup.Addon>
					</InputGroup.Root>
				</Field.Field>
			{/if}
		</Field.FieldGroup>
		<Dialog.Footer>
			<Button
				variant="ghost"
				class="rounded-md text-xs"
				onclick={() => (dialogOpen = false)}
			>
				cancel
			</Button>
			<Button
				class="rounded-md text-xs"
				onclick={commitDraft}
				disabled={nameError !== null}
			>
				{editingId === null ? "add" : "save"}
			</Button>
		</Dialog.Footer>
	</Dialog.Content>
</Dialog.Root>
