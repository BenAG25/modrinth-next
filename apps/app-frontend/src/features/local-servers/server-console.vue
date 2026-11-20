<script setup lang="ts">
import { SendIcon, TerminalSquareIcon } from '@modrinth/assets'
import { Button, injectNotificationManager, Input } from '@modrinth/ui'
import { nextTick, onBeforeUnmount, onMounted, ref, watch } from 'vue'

import {
	getLocalServerLogs,
	onLocalServerLog,
	sendLocalServerCommand,
} from '@/platform/app-lib/local-servers/commands'
import type { LocalServerStatus } from '@/platform/app-lib/local-servers/types'

const props = defineProps<{ serverId: string; status: LocalServerStatus }>()
const { handleError } = injectNotificationManager()

const MAX_LINES = 2000
const lines = ref<string[]>([])
const command = ref<string | number | undefined>('')
const history = ref<string[]>([])
const historyIndex = ref(-1)
const scroller = ref<HTMLElement | null>(null)
const stickToBottom = ref(true)
let unlisten: (() => void) | null = null

function lineClass(line: string) {
	if (line.startsWith('[Modrinth Next]')) return 'text-brand'
	if (line.startsWith('> ')) return 'text-contrast font-semibold'
	if (/\b(ERROR|SEVERE|FATAL)\b|Exception/.test(line)) return 'text-red'
	if (/\bWARN(ING)?\b/.test(line)) return 'text-orange'
	return 'text-primary'
}

function onScroll() {
	const el = scroller.value
	if (!el) return
	stickToBottom.value = el.scrollHeight - el.scrollTop - el.clientHeight < 40
}

async function scrollToBottom() {
	await nextTick()
	if (scroller.value && stickToBottom.value) {
		scroller.value.scrollTop = scroller.value.scrollHeight
	}
}

async function load() {
	lines.value = await getLocalServerLogs(props.serverId)
	stickToBottom.value = true
	await scrollToBottom()
}

async function send() {
	const cmd = String(command.value ?? '').trim()
	if (!cmd) return
	history.value.unshift(cmd)
	historyIndex.value = -1
	command.value = ''
	try {
		await sendLocalServerCommand(props.serverId, cmd)
	} catch (e) {
		handleError(e as Error)
	}
}

function onKeydown(e: KeyboardEvent) {
	if (e.key === 'ArrowUp' && history.value.length) {
		e.preventDefault()
		historyIndex.value = Math.min(historyIndex.value + 1, history.value.length - 1)
		command.value = history.value[historyIndex.value]
	} else if (e.key === 'ArrowDown') {
		e.preventDefault()
		historyIndex.value = Math.max(historyIndex.value - 1, -1)
		command.value = historyIndex.value === -1 ? '' : history.value[historyIndex.value]
	}
}

onMounted(async () => {
	unlisten = await onLocalServerLog(({ id, line }) => {
		if (id !== props.serverId) return
		lines.value.push(line)
		if (lines.value.length > MAX_LINES) lines.value.splice(0, lines.value.length - MAX_LINES)
		scrollToBottom()
	})
	await load()
})
onBeforeUnmount(() => unlisten?.())
watch(() => props.serverId, load)
</script>

<template>
	<div class="flex h-full min-h-0 flex-col gap-3">
		<div
			ref="scroller"
			class="min-h-[300px] flex-1 overflow-y-auto rounded-2xl border border-solid border-surface-4 bg-surface-1 p-4 font-mono text-xs leading-relaxed"
			@scroll="onScroll"
		>
			<div v-if="lines.length === 0" class="text-secondary">
				Noch keine Ausgabe. Starte den Server, um die Konsole zu sehen.
			</div>
			<div
				v-for="(line, i) in lines"
				:key="i"
				class="whitespace-pre-wrap break-all"
				:class="lineClass(line)"
			>
				{{ line }}
			</div>
		</div>
		<form class="flex gap-2" @submit.prevent="send">
			<Input
				v-model="command"
				:icon="TerminalSquareIcon"
				wrapper-class="min-w-0 flex-1"
				input-class="font-mono"
				:input-attrs="{ onKeydown }"
				:placeholder="
					status === 'stopped'
						? 'Server ist gestoppt'
						: 'Befehl eingeben, z. B. op DeinName oder time set day'
				"
				:disabled="status === 'stopped'"
			/>
			<Button
				type="colored"
				color="brand"
				native-type="submit"
				:disabled="status === 'stopped' || !String(command ?? '').trim()"
			>
				<SendIcon /> Senden
			</Button>
		</form>
	</div>
</template>
