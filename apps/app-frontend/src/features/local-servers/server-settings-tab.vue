<script setup lang="ts">
import { CheckIcon, SpinnerIcon, TrashIcon } from '@modrinth/assets'
import { Button, Combobox, injectNotificationManager, Input, Slider, Toggle } from '@modrinth/ui'
import { onMounted, ref, watch } from 'vue'

import {
	deleteLocalServer,
	getLocalServerProperties,
	setLocalServerProperties,
	updateLocalServerSettings,
} from '@/platform/app-lib/local-servers/commands'
import type { LocalServer } from '@/platform/app-lib/local-servers/types'

const props = defineProps<{ server: LocalServer }>()
const emit = defineEmits<{ updated: [server: LocalServer]; deleted: [] }>()
const { handleError, addNotification } = injectNotificationManager()

const name = ref(props.server.name)
const memoryGb = ref(props.server.memory_mb / 1024)
const jvmArgs = ref(props.server.jvm_args.join(' '))

const motd = ref('')
const port = ref<string | number | undefined>(25565)
const maxPlayers = ref<string | number | undefined>(20)
const gamemode = ref('survival')
const difficulty = ref('easy')
const onlineMode = ref(true)
const pvp = ref(true)
const whitelist = ref(false)
const allowFlight = ref(false)
const commandBlocks = ref(false)
const seed = ref('')

const saving = ref(false)
const confirmDelete = ref(false)

const gamemodes = [
	{ value: 'survival', label: 'Überleben' },
	{ value: 'creative', label: 'Kreativ' },
	{ value: 'adventure', label: 'Abenteuer' },
	{ value: 'spectator', label: 'Zuschauer' },
]
const difficulties = [
	{ value: 'peaceful', label: 'Friedlich' },
	{ value: 'easy', label: 'Einfach' },
	{ value: 'normal', label: 'Normal' },
	{ value: 'hard', label: 'Schwer' },
]

const bool = (v: string | undefined, fallback: boolean) =>
	v === undefined ? fallback : v.toLowerCase() === 'true'

async function load() {
	name.value = props.server.name
	memoryGb.value = props.server.memory_mb / 1024
	jvmArgs.value = props.server.jvm_args.join(' ')
	try {
		const p = await getLocalServerProperties(props.server.id)
		motd.value = p['motd'] ?? ''
		port.value = Number(p['server-port'] ?? 25565)
		maxPlayers.value = Number(p['max-players'] ?? 20)
		gamemode.value = p['gamemode'] ?? 'survival'
		difficulty.value = p['difficulty'] ?? 'easy'
		onlineMode.value = bool(p['online-mode'], true)
		pvp.value = bool(p['pvp'], true)
		whitelist.value = bool(p['white-list'], false)
		allowFlight.value = bool(p['allow-flight'], false)
		commandBlocks.value = bool(p['enable-command-block'], false)
		seed.value = p['level-seed'] ?? ''
	} catch (e) {
		handleError(e as Error)
	}
}

async function save() {
	saving.value = true
	try {
		await setLocalServerProperties(props.server.id, {
			motd: motd.value,
			'server-port': String(port.value),
			'max-players': String(maxPlayers.value),
			gamemode: gamemode.value,
			difficulty: difficulty.value,
			'online-mode': String(onlineMode.value),
			pvp: String(pvp.value),
			'white-list': String(whitelist.value),
			'allow-flight': String(allowFlight.value),
			'enable-command-block': String(commandBlocks.value),
			'level-seed': seed.value,
		})
		const updated = await updateLocalServerSettings(props.server.id, {
			name: name.value,
			memory_mb: Math.round(memoryGb.value * 1024),
			jvm_args: jvmArgs.value.split(/\s+/).filter(Boolean),
		})
		emit('updated', updated)
		addNotification({
			type: 'success',
			title: 'Gespeichert',
			text:
				props.server.status !== 'stopped'
					? 'Starte den Server neu, damit die Änderungen aktiv werden.'
					: undefined,
		})
	} catch (e) {
		handleError(e as Error)
	} finally {
		saving.value = false
	}
}

async function remove() {
	if (!confirmDelete.value) {
		confirmDelete.value = true
		return
	}
	try {
		await deleteLocalServer(props.server.id)
		emit('deleted')
	} catch (e) {
		handleError(e as Error)
	}
}

onMounted(load)
watch(() => props.server.id, load)
</script>

<template>
	<div class="flex max-w-3xl flex-col gap-6">
		<section
			class="flex flex-col gap-4 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-5"
		>
			<h2 class="m-0 text-lg font-bold text-contrast">Allgemein</h2>
			<label class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Name</span>
				<Input v-model="name" :maxlength="64" wrapper-class="w-full" />
			</label>
			<label class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Nachricht in der Serverliste (MOTD)</span>
				<Input v-model="motd" :maxlength="120" wrapper-class="w-full" />
			</label>
			<div class="grid grid-cols-2 gap-4">
				<label class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">Port</span>
					<Input v-model="port" type="number" :min="1024" :max="65535" wrapper-class="w-full" />
				</label>
				<label class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">Max. Spieler</span>
					<Input v-model="maxPlayers" type="number" :min="1" :max="1000" wrapper-class="w-full" />
				</label>
			</div>
		</section>

		<section
			class="flex flex-col gap-4 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-5"
		>
			<h2 class="m-0 text-lg font-bold text-contrast">Gameplay</h2>
			<div class="grid grid-cols-2 gap-4">
				<div class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">Spielmodus</span>
					<Combobox v-model="gamemode" :options="gamemodes" />
				</div>
				<div class="flex flex-col gap-2">
					<span class="font-semibold text-contrast">Schwierigkeit</span>
					<Combobox v-model="difficulty" :options="difficulties" />
				</div>
			</div>
			<label class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Welt-Seed (nur für neue Welten)</span>
				<Input v-model="seed" placeholder="Zufällig" wrapper-class="w-full" />
			</label>
			<div class="flex flex-col gap-3">
				<div class="flex items-center justify-between gap-4">
					<div>
						<div class="font-semibold text-contrast">PvP</div>
						<div class="text-sm text-secondary">Spieler können sich gegenseitig angreifen.</div>
					</div>
					<Toggle v-model="pvp" />
				</div>
				<div class="flex items-center justify-between gap-4">
					<div>
						<div class="font-semibold text-contrast">Fliegen erlauben</div>
						<div class="text-sm text-secondary">Wichtig für manche Mods (z. B. Jetpacks).</div>
					</div>
					<Toggle v-model="allowFlight" />
				</div>
				<div class="flex items-center justify-between gap-4">
					<div class="font-semibold text-contrast">Befehlsblöcke</div>
					<Toggle v-model="commandBlocks" />
				</div>
			</div>
		</section>

		<section
			class="flex flex-col gap-4 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-5"
		>
			<h2 class="m-0 text-lg font-bold text-contrast">Zugang</h2>
			<div class="flex items-center justify-between gap-4">
				<div>
					<div class="font-semibold text-contrast">Online-Modus</div>
					<div class="text-sm text-secondary">
						Prüft Minecraft-Accounts bei Mojang. Nur ausschalten, wenn du weißt, was du tust.
					</div>
				</div>
				<Toggle v-model="onlineMode" />
			</div>
			<div class="flex items-center justify-between gap-4">
				<div>
					<div class="font-semibold text-contrast">Whitelist</div>
					<div class="text-sm text-secondary">
						Nur freigegebene Spieler dürfen beitreten (Konsole: <code>whitelist add Name</code>).
					</div>
				</div>
				<Toggle v-model="whitelist" />
			</div>
		</section>

		<section
			class="flex flex-col gap-4 rounded-2xl border border-solid border-surface-4 bg-bg-raised p-5"
		>
			<h2 class="m-0 text-lg font-bold text-contrast">Leistung</h2>
			<div class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Arbeitsspeicher: {{ memoryGb }} GB</span>
				<Slider v-model="memoryGb" :min="1" :max="32" :step="0.5" unit=" GB" />
			</div>
			<label class="flex flex-col gap-2">
				<span class="font-semibold text-contrast">Zusätzliche Java-Argumente</span>
				<Input
					v-model="jvmArgs"
					input-class="font-mono"
					placeholder="-XX:+UseG1GC"
					wrapper-class="w-full"
				/>
			</label>
			<div class="text-sm text-secondary">Java {{ server.java_major }} · {{ server.path }}</div>
		</section>

		<div class="flex flex-wrap justify-between gap-2">
			<Button
				type="outlined"
				color="red"
				:disabled="server.status !== 'stopped'"
				@click="remove"
				@blur="confirmDelete = false"
			>
				<TrashIcon />
				{{ confirmDelete ? 'Wirklich löschen? Klicke erneut' : 'Server löschen' }}
			</Button>
			<Button type="colored" color="brand" :disabled="saving" @click="save">
				<SpinnerIcon v-if="saving" class="animate-spin" />
				<CheckIcon v-else />
				Speichern
			</Button>
		</div>
	</div>
</template>
