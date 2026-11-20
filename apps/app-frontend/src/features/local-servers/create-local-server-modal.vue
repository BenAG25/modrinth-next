<script setup lang="ts">
import { PlusIcon, SpinnerIcon } from '@modrinth/assets'
import {
	Admonition,
	Avatar,
	Button,
	Checkbox,
	Combobox,
	injectNotificationManager,
	Input,
	NewModal,
	SegmentedControl,
	Slider,
} from '@modrinth/ui'
import { computed, onBeforeUnmount, ref, watch } from 'vue'

import { getInstanceIconUrl, list } from '@/helpers/instance'
import type { GameInstance } from '@/helpers/types'
import {
	createLocalServer,
	getLocalServer,
	getLocalServerGameVersions,
	getLocalServerLoaderVersions,
	linkLocalServerInstance,
	onLocalServerProgress,
} from '@/platform/app-lib/local-servers/commands'
import type { LocalServer, LocalServerLoader } from '@/platform/app-lib/local-servers/types'
import { openExternalUrl } from '@/platform/external-links'
import { injectAppEvents } from '@/providers/app-events'

import { LOADERS } from './loaders'
import {
	createProfileForServer,
	describeSync,
	serverLoaderFor,
	sharesMods,
	syncProfileToServer,
} from './profile-link'

const emit = defineEmits<{ created: [server: LocalServer] }>()
const { handleError, addNotification } = injectNotificationManager()
const appEvents = injectAppEvents()

const modal = ref<InstanceType<typeof NewModal> | null>(null)

const MODES = ['Neuer Server', 'Aus Spielprofil'] as const
const mode = ref<(typeof MODES)[number]>('Neuer Server')
const fromProfile = computed(() => mode.value === 'Aus Spielprofil')

const name = ref<string | number | undefined>('')
const loader = ref<LocalServerLoader>('fabric')
const gameVersion = ref<string | undefined>()
const loaderVersion = ref<string>('')
const memoryGb = ref(4)
const port = ref<string | number | undefined>(25565)
const acceptEula = ref(false)
const showSnapshots = ref(false)
const createProfile = ref(true)

const instances = ref<GameInstance[]>([])
const instanceId = ref<string | undefined>()
const selectedInstance = computed(() => instances.value.find((i) => i.id === instanceId.value))

const gameVersions = ref<string[]>([])
const loaderVersions = ref<string[]>([])
const loadingVersions = ref(false)
const creating = ref(false)
const progress = ref('')

let unlistenProgress: (() => void) | null = null

const usesLoaderVersion = computed(() => ['fabric', 'forge', 'neoforge'].includes(loader.value))
const gameVersionOptions = computed(() => gameVersions.value.map((v) => ({ value: v, label: v })))
const loaderVersionOptions = computed(() => [
	{ value: '', label: 'Neueste (empfohlen)' },
	...loaderVersions.value.map((v) => ({ value: v, label: v })),
])
const instanceOptions = computed(() =>
	instances.value.map((i) => ({
		value: i.id,
		label: i.name,
		subLabel: `${i.loader} ${i.game_version}`,
	})),
)

const canCreate = computed(
	() =>
		!!String(name.value ?? '').trim() &&
		!!gameVersion.value &&
		acceptEula.value &&
		!creating.value &&
		(!fromProfile.value || !!selectedInstance.value),
)

async function loadGameVersions() {
	loadingVersions.value = true
	try {
		const versions = await getLocalServerGameVersions(loader.value, showSnapshots.value)
		gameVersions.value = versions
		if (!gameVersion.value || !versions.includes(gameVersion.value)) {
			gameVersion.value = fromProfile.value ? selectedInstance.value?.game_version : versions[0]
		}
	} catch (e) {
		gameVersions.value = []
		handleError(e as Error)
	} finally {
		loadingVersions.value = false
	}
}

async function loadLoaderVersions() {
	loaderVersions.value = []
	if (!usesLoaderVersion.value || !gameVersion.value) {
		loaderVersion.value = ''
		return
	}
	try {
		loaderVersions.value = await getLocalServerLoaderVersions(loader.value, gameVersion.value)
	} catch {
		loaderVersions.value = []
	}
	// Prefer the loader version of the selected profile so mods match exactly.
	const wanted = fromProfile.value ? selectedInstance.value?.loader_version : undefined
	const match = wanted
		? loaderVersions.value.find((v) => v === wanted || v.endsWith(`-${wanted}`))
		: undefined
	loaderVersion.value = match ?? ''
}

watch([loader, showSnapshots], loadGameVersions)
watch([loader, gameVersion, instanceId], loadLoaderVersions)

function applyInstance(instance: GameInstance | undefined) {
	if (!instance) return
	const serverLoader = serverLoaderFor(instance.loader)
	if (serverLoader) loader.value = serverLoader
	gameVersion.value = instance.game_version
	if (!String(name.value ?? '').trim()) name.value = `${instance.name} Server`
	if (gameVersions.value.length && !gameVersions.value.includes(instance.game_version)) {
		gameVersions.value = [instance.game_version, ...gameVersions.value]
	}
}
watch(selectedInstance, applyInstance)

async function show(options: { fromInstance?: string } = {}) {
	name.value = ''
	acceptEula.value = false
	progress.value = ''
	creating.value = false
	createProfile.value = true
	instanceId.value = undefined
	mode.value = options.fromInstance ? 'Aus Spielprofil' : 'Neuer Server'
	modal.value?.show()

	instances.value = (await list()).filter((i) => serverLoaderFor(i.loader) !== null)
	if (options.fromInstance) instanceId.value = options.fromInstance
	if (gameVersions.value.length === 0) await loadGameVersions()
}

async function create() {
	if (!canCreate.value || !gameVersion.value) return
	creating.value = true
	progress.value = 'Server wird erstellt…'
	unlistenProgress?.()
	unlistenProgress = await onLocalServerProgress((event) => {
		progress.value = event.message
	})
	try {
		let server = await createLocalServer({
			name: String(name.value).trim(),
			loader: loader.value,
			game_version: gameVersion.value,
			loader_version: loaderVersion.value || null,
			memory_mb: Math.round(memoryGb.value * 1024),
			port: Number(port.value) || 25565,
			accept_eula: acceptEula.value,
		})

		if (fromProfile.value && selectedInstance.value) {
			server = await linkLocalServerInstance(server.id, selectedInstance.value.id)
			if (sharesMods(server.loader)) {
				progress.value = 'Mods aus dem Profil werden installiert…'
				const report = await syncProfileToServer(server, selectedInstance.value.id)
				addNotification({ type: 'success', title: 'Server erstellt', text: describeSync(report) })
			}
		} else if (createProfile.value) {
			progress.value = 'Passendes Spielprofil wird erstellt…'
			try {
				await createProfileForServer(server, appEvents)
			} catch (e) {
				handleError(e as Error)
			}
		}

		modal.value?.hide()
		emit('created', await getLocalServer(server.id))
	} catch (e) {
		handleError(e as Error)
	} finally {
		creating.value = false
		unlistenProgress?.()
		unlistenProgress = null
	}
}

onBeforeUnmount(() => unlistenProgress?.())

defineExpose({ show })
</script>

<template>
	<NewModal ref="modal" header="Lokalen Server erstellen" max-width="600px" width="600px">
		<div class="flex flex-col gap-5">
			<SegmentedControl
				v-model="mode"
				:items="[...MODES]"
				aria-label="Art des Servers"
				full-width
				equal-width
				:disabled="creating"
			/>

			<!-- From profile -->
			<div v-if="fromProfile" class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Spielprofil</span>
				<div v-if="instances.length === 0" class="text-sm text-secondary">
					Du hast noch kein passendes Spielprofil (Vanilla, Fabric, Forge oder NeoForge).
				</div>
				<Combobox
					v-else
					v-model="instanceId"
					:options="instanceOptions"
					searchable
					placeholder="Profil wählen"
					:disabled="creating"
					:max-height="300"
				/>
				<div
					v-if="selectedInstance"
					class="flex items-center gap-3 rounded-2xl border border-solid border-surface-4 bg-surface-2 p-3"
				>
					<Avatar
						:src="getInstanceIconUrl(selectedInstance.icon_path) ?? undefined"
						:tint-by="selectedInstance.id"
						size="40px"
						pad-transparent-corners
					/>
					<div class="text-sm text-secondary">
						Der Server bekommt dieselbe Version und denselben Loader. Alle Mods, die auch auf
						Servern laufen, werden automatisch installiert. Reine Client-Mods (z. B. Shader oder
						Minimaps) werden übersprungen.
					</div>
				</div>
			</div>

			<label class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Name</span>
				<Input
					v-model="name"
					:maxlength="64"
					placeholder="Mein Server"
					:disabled="creating"
					wrapper-class="w-full"
				/>
			</label>

			<div v-if="!fromProfile" class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Server-Software</span>
				<div class="grid grid-cols-5 gap-2">
					<button
						v-for="l in LOADERS"
						:key="l.id"
						type="button"
						class="rounded-xl border-2 border-solid px-2 py-3 text-sm font-bold transition-colors"
						:class="
							loader === l.id
								? 'border-brand bg-brand-highlight text-contrast'
								: 'border-surface-4 bg-surface-2 text-primary hover:bg-surface-3'
						"
						:disabled="creating"
						@click="loader = l.id"
					>
						{{ l.label }}
					</button>
				</div>
				<span class="text-sm text-secondary">
					{{ LOADERS.find((l) => l.id === loader)?.description }}
				</span>
			</div>

			<div class="grid grid-cols-2 gap-4">
				<div class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">Minecraft-Version</span>
					<Combobox
						v-model="gameVersion"
						:options="gameVersionOptions"
						searchable
						:disabled="creating || loadingVersions || fromProfile"
						:placeholder="loadingVersions ? 'Lädt…' : 'Version wählen'"
						:max-height="300"
					/>
					<Checkbox
						v-if="!fromProfile"
						v-model="showSnapshots"
						label="Snapshots anzeigen"
						:disabled="creating"
					/>
				</div>
				<div v-if="usesLoaderVersion" class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">Loader-Version</span>
					<Combobox
						v-model="loaderVersion"
						:options="loaderVersionOptions"
						searchable
						:disabled="creating"
						:max-height="300"
					/>
				</div>
			</div>

			<div class="grid grid-cols-[1fr_auto] items-end gap-4">
				<div class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">Arbeitsspeicher</span>
					<Slider
						v-model="memoryGb"
						:min="1"
						:max="16"
						:step="0.5"
						unit=" GB"
						:disabled="creating"
					/>
				</div>
				<label class="flex w-28 flex-col gap-2">
					<span class="font-semibold text-contrast">Port</span>
					<Input
						v-model="port"
						type="number"
						:min="1024"
						:max="65535"
						:disabled="creating"
						wrapper-class="w-full"
					/>
				</label>
			</div>

			<div
				class="flex flex-col gap-3 rounded-2xl border border-solid border-surface-4 bg-surface-2 p-4"
			>
				<Checkbox
					v-if="!fromProfile"
					v-model="createProfile"
					label="Passendes Spielprofil erstellen (gleiche Version, Loader und Mods)"
					:disabled="creating"
				/>
				<div class="flex flex-wrap items-center gap-x-2">
					<Checkbox
						v-model="acceptEula"
						label="Ich akzeptiere die Minecraft-EULA"
						:disabled="creating"
					/>
					<button
						type="button"
						class="text-sm text-link hover:underline"
						@click="openExternalUrl('https://aka.ms/MinecraftEULA')"
					>
						(lesen)
					</button>
				</div>
			</div>

			<Admonition v-if="creating" type="info" :header="progress">
				Beim ersten Mal werden Server-Dateien und ggf. Java heruntergeladen.
			</Admonition>

			<div class="flex justify-end gap-2">
				<Button type="quiet" :disabled="creating" @click="modal?.hide()">Abbrechen</Button>
				<Button type="colored" color="brand" :disabled="!canCreate" @click="create">
					<SpinnerIcon v-if="creating" class="animate-spin" />
					<PlusIcon v-else />
					Server erstellen
				</Button>
			</div>
		</div>
	</NewModal>
</template>
