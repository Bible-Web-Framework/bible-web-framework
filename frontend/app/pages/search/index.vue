<script lang="ts" setup>
import type { FunctionalComponent, VNode } from 'vue'
import type { LocationQuery } from 'vue-router'
import {
  getShortBookName,
  isFullChapter,
  type ApiV1,
  type BibleReference,
  type ChapterReference,
} from '~/bwfApi'
import UsjContentsRenderer from '~/components/UsjContentsRenderer.vue'
import {
  MACHINE_REFERENCE_REGEX,
  normalizeNoteCallers,
  walkUsj,
  type Book,
  type ParaContent,
  type UsjContent,
} from '~/usj'
import { NuxtLink } from '#components'
import { mount } from '@vue/test-utils'

const config = useRuntimeConfig()
const { data: biblesData } = await useFetch<ApiV1['bibles']>('/v1/bibles', {
  baseURL: config.public.apiRootUrl,
})

const route = useRoute()
const router = useRouter()

const bible = computed(() =>
  (route.query.bible || biblesData.value?.default_bible || '').toString(),
)
const { data: booksData } = await useFetch<ApiV1['bible']['books']>(
  () => `/v1/bible/${bible.value}/books`,
  {
    baseURL: config.public.apiRootUrl,
  },
)
const bibleInfo = computed(() => biblesData.value?.bibles?.[bible.value])

const query = computed(() => (route.query.q || '').toString())
const page = computed({
  get: () => Math.max(Math.round(+(route.query.page || '1').toString() || 1), 1),
  set: (page) => router.push({ query: { ...route.query, page } }),
})
const resultsPerPage = computed({
  get: () => Math.min(Math.max(Math.round(+(route.query.count || '50').toString() || 50), 1), 250),
  set: (count) => {
    const oldCount = resultsPerPage.value
    const oldStart = (page.value - 1) * oldCount
    router.push({ query: { ...route.query, page: Math.floor(oldStart / count) + 1, count } })
  },
})

const { data: searchData } = await useAsyncData(
  'searchResults',
  async (_nuxtApp, { signal }) => {
    const response = await $fetch<ApiV1['bible']['search']>(`/v1/bible/${bible.value}/search`, {
      baseURL: config.public.apiRootUrl,
      query: {
        term: query.value,
        start: (page.value - 1) * resultsPerPage.value,
        count: resultsPerPage.value,
      },
      signal,
    })

    let noteId = 0
    for (const reference of response.references) {
      if ('content' in reference && reference.content) {
        noteId = normalizeNoteCallers(reference.content, noteId)
      }
    }

    for (const reference of response.references) {
      if (!('content' in reference) || !reference.content || reference.highlights) {
        continue
      }
      const chapterIndex = reference.content.findIndex((el) => el.type === 'chapter')
      const chapter = reference.content[chapterIndex]!
      if (chapterIndex === -1 || chapter.type !== 'chapter') {
        continue
      }
      const isCl = (el: UsjContent) => el.type === 'para' && el.marker === 'cl'
      const clIndexBefore = reference.content.findIndex((el, i) => i < chapterIndex && isCl(el))
      const clIndexAfter = reference.content.findIndex((el, i) => i > chapterIndex && isCl(el))
      if (clIndexAfter !== -1) {
        const cl = reference.content.splice(clIndexAfter, 1)[0]!
        if (clIndexBefore !== -1) {
          reference.content.splice(clIndexBefore, 1, cl)
        } else {
          reference.content.splice(0, 0, cl)
        }
      } else if (clIndexBefore !== -1) {
        const cl = reference.content[clIndexBefore]!
        if (cl.type !== 'para') {
          continue
        }
        cl.content = (cl.content ?? []).concat(` ${chapter.pubnumber ?? chapter.number}`)
      } else {
        reference.content.splice(0, 0, {
          type: 'para',
          marker: 'cl',
          content: [
            `${getShortBookName(reference.translated_book_info, reference.reference.book)} ${chapter.pubnumber ?? chapter.number}`,
          ],
        })
      }
    }

    return {
      results: response,
      noteCount: noteId,
    }
  },
  {
    watch: [bible, query, page, resultsPerPage],
  },
)
const searchResults = computed(() => searchData.value?.results)

const pageCount = computed(() => {
  if (!searchResults.value || searchResults.value.response_type !== 'search_results') {
    return 0
  }
  return Math.ceil(searchResults.value.total_results / resultsPerPage.value)
})

function formatReference(ref: BibleReference | ChapterReference, overrideBookName?: string) {
  let q =
    overrideBookName ??
    getShortBookName(booksData.value?.books[ref.book]?.translated_book_info ?? null, ref.book)
  q += ` ${ref.chapter}`
  if ('verses' in ref) {
    const [start, end] = ref.verses
    q += `:${start}`
    if (end > start) {
      q += `-${end}`
    }
  }
  return q
}

const newQuery = ref(query.value)
function newQueryParamsForSearch(
  q: string | BibleReference | ChapterReference,
  normalize: boolean = false,
) {
  if (typeof q !== 'string') {
    q = formatReference(q)
    normalize = false
  }
  if (normalize && booksData.value && MACHINE_REFERENCE_REGEX.test(q)) {
    for (const [book, translation] of Object.entries(booksData.value.books)) {
      q = q.replaceAll(book, getShortBookName(translation.translated_book_info, book as Book))
    }
  }
  const newQueryParams: LocationQuery = { ...route.query, q }
  delete newQueryParams['page']
  return newQueryParams
}
function search() {
  router.push({ query: newQueryParamsForSearch(newQuery.value) })
}

const newBook = ref<Book | null>(null)
const newChapter = ref<number | null>(null)
const newBible = ref(bible.value)

function computeCurrentBookAndChapter() {
  const results = searchResults.value
  if (
    results === undefined ||
    results.response_type === 'search_results' ||
    results.total_results !== 1
  ) {
    newBook.value = null
    return
  }
  const reference = results.references.find((x) => 'reference' in x)
  if (reference === undefined) {
    newBook.value = null
    return
  }
  newBook.value = reference.reference.book
  newChapter.value = reference.reference.chapter
}
watch(searchResults, computeCurrentBookAndChapter)
computeCurrentBookAndChapter()

watch([query, bible], () => {
  newQuery.value = query.value
  newBible.value = bible.value
})

function directGo() {
  const book = newBook.value
  const chapter = newChapter.value
  if (book === null || chapter === null) {
    return
  }
  router.push({
    query: newQueryParamsForSearch({ book, chapter, translated_book_info: null }),
  })
}

function changeBible() {
  const newParams: LocationQuery = { ...route.query, bible: newBible.value }
  const currentResults = searchResults.value
  if (currentResults && currentResults.response_type === 'scripture_passages') {
    delete newParams['page']
    let newQuery = ''
    for (const result of currentResults.references) {
      if (newQuery.length > 0) {
        newQuery += '; '
      }
      if ('invalid_reference' in result) {
        newQuery += result.invalid_reference
      } else {
        newQuery += formatReference(
          result.reference,
          biblesData.value?.bibles?.[newBible.value]?.simple_book_names?.[result.reference.book],
        )
      }
    }
    newParams.q = newQuery
  }
  router.push({
    query: newParams,
  })
}

const NotesRenderer: FunctionalComponent<{ contents: ParaContent[] }> = ({ contents }) => {
  const notes: VNode[] = []
  walkUsj(contents, (element) => {
    if (typeof element === 'string' || element.type !== 'note') {
      return true
    }
    notes.push(
      h('div', { class: 'note-contents' }, [
        h(
          'a',
          {
            class: 'usj-content f',
            name: `note-contents-${element.caller}`,
            href: `#note-source-${element.caller}`,
          },
          [element.caller],
        ),
        h(UsjContentsRenderer, {
          contents: element.content,
          textDirection: bibleTextDirection.value,
          ignoredContentTypes: ['note'],
          generateSearchQuery: newQueryParamsForSearch,
        }),
      ]),
    )
    return false
  })
  return notes
}
NotesRenderer.props = {
  contents: {
    type: Array,
    required: true,
  },
}

const scannedBooks = ref<number>()
const totalBooks = computed(() => {
  if (!booksData.value) {
    return 0
  }
  return Object.keys(booksData.value.books).length
})
const currentlyScanning = ref<string>()
async function checkForUnimplementedMarkers() {
  if (!booksData.value) {
    return alert('No bibles loaded!')
  }
  scannedBooks.value = 0

  let totalMissing = 0
  let missingReferences = ''
  for (const [book, bookInfo] of Object.entries(booksData.value.books)) {
    currentlyScanning.value = `${book}/${getShortBookName(bookInfo.translated_book_info, book as Book)}`

    const bookData = await $fetch<ApiV1['bible']['book']>(`/v1/bible/${bible.value}/book/${book}`, {
      baseURL: config.public.apiRootUrl,
    })

    function scan(contents: UsjContent[]) {
      const first = contents[0]
      if (first?.type !== 'chapter') return
      const rendered = mount(UsjContentsRenderer, {
        props: {
          contents,
          textDirection: bibleTextDirection.value,
        },
      })
      const missing = rendered.findAllComponents({
        name: 'UnimplementedMarker',
      })
      if (missing.length > 0) {
        totalMissing += missing.length
        console.warn(
          'Found missing in chapter',
          book,
          first.number,
          missing.map((dom) => {
            const text: string = dom.element.innerText
            return text.substring(text.indexOf(':') + 2, text.length - 1)
          }),
        )

        if (missingReferences) {
          missingReferences += ';'
        }
        missingReferences += `${book}${first.number}`
      }
    }

    let prevChapter = bookData.content.findIndex((u) => u.type === 'chapter')
    while (prevChapter < bookData.content.length) {
      let nextChapter = bookData.content
        .slice(prevChapter + 1)
        .findIndex((u) => u.type === 'chapter')
      if (nextChapter === -1) {
        scan(bookData.content.slice(prevChapter))
        break
      }
      nextChapter += prevChapter + 1
      scan(bookData.content.slice(prevChapter, nextChapter))
      prevChapter = nextChapter
    }

    scannedBooks.value++
  }
  scannedBooks.value = undefined
  currentlyScanning.value = undefined

  console.info('Found unimplemented markers in', totalMissing, 'places')

  if (totalMissing) {
    alert(`Found unimplemented markers in ${totalMissing} places`)
    newQuery.value = missingReferences
    search()
  } else {
    alert('No unimplemented markers found!')
  }
}

const bibleTextDirection = computed(() => bibleInfo.value?.text_direction ?? 'auto')
</script>

<template>
  <div>
    <DevOnly>
      <span>
        <button @click="checkForUnimplementedMarkers">
          Check for unimplemented markers in selected bible
        </button>
        <template v-if="scannedBooks !== undefined">
          Scanned {{ scannedBooks }}/{{ totalBooks }} books
        </template>
        <template v-if="currentlyScanning !== undefined">
          Currently scanning {{ currentlyScanning }}
        </template>
      </span>
    </DevOnly>

    <h1>Search Page</h1>

    <div class="search-area">
      <input
        v-model="newQuery"
        placeholder="Enter search term"
        :dir="bibleTextDirection"
        class="search-box"
        @keyup.enter="search"
      />
      <button @click="search">Search</button>
      <select
        v-model="newBook"
        :dir="bibleTextDirection"
        class="book-box"
        @change="newChapter = null"
      >
        <option :value="null">----</option>
        <template v-if="booksData">
          <!-- TODO: Make the order here match the book_order field -->
          <option v-for="(info, book) in booksData.books" :key="book" :value="book">
            {{ getShortBookName(info.translated_book_info, book) }}
          </option>
        </template>
      </select>
      <select v-model="newChapter" @change="directGo">
        <option :value="null">--</option>
        <template v-if="booksData && newBook">
          <option
            v-for="chapter in booksData.books[newBook].chapters"
            :key="chapter.number"
            :value="chapter.number"
          >
            {{ chapter.pub_number ?? chapter.number }}
          </option>
        </template>
      </select>
      <select v-if="biblesData" v-model="newBible" class="bible-box" @change="changeBible">
        <option v-for="(info, id) in biblesData.bibles" :key="id" :value="id">
          {{ info.display_name ?? id.toLocaleUpperCase() }}
        </option>
      </select>
      <span v-else />
    </div>

    <template v-if="query && searchResults?.response_type === 'search_results'">
      <template v-if="pageCount > 1">
        Page:
        <select v-model="page">
          <option v-for="number in pageCount" :key="number" :value="number">
            {{ number }}
          </option>
        </select>
      </template>
      Results per page:
      <select v-model="resultsPerPage">
        <option :value="50">50</option>
        <option :value="100">100</option>
        <option :value="150">150</option>
        <option :value="200">200</option>
        <option :value="250">250</option>
      </select>
    </template>

    <div v-if="searchResults">
      <template v-if="searchResults.response_type === 'scripture_passages'">
        <template
          v-for="(reference, referenceIndex) in searchResults.references"
          :key="referenceIndex"
        >
          <hr v-if="referenceIndex > 0" />
          <template v-if="'content' in reference">
            <template v-if="reference.content">
              <div
                v-if="
                  isFullChapter(reference.reference) &&
                  (reference.previous_chapter || reference.next_chapter)
                "
                :dir="bibleTextDirection"
                class="sided-nav"
              >
                <NuxtLink
                  v-if="reference.previous_chapter"
                  :to="{
                    query: newQueryParamsForSearch(reference.previous_chapter),
                  }"
                  >❮ {{ formatReference(reference.previous_chapter) }}</NuxtLink
                >
                <div v-else />
                <NuxtLink
                  v-if="reference.next_chapter"
                  :to="{
                    query: newQueryParamsForSearch(reference.next_chapter),
                  }"
                  >{{ formatReference(reference.next_chapter) }} ❯</NuxtLink
                >
                <div v-else />
              </div>
              <div class="usj-container">
                <UsjContentsRenderer
                  :contents="reference.content"
                  :text-direction="bibleTextDirection"
                  :generate-search-query="newQueryParamsForSearch"
                />
              </div>
              <div v-if="!isFullChapter(reference.reference)" class="center-nav">
                <NuxtLink
                  :to="{
                    query: newQueryParamsForSearch({
                      book: reference.reference.book,
                      chapter: reference.reference.chapter,
                      translated_book_info: null,
                    }),
                  }"
                  >View full chapter</NuxtLink
                >
              </div>
            </template>
            <p v-else class="error">
              No scripture passage found for {{ formatReference(reference.reference) }}
            </p>
          </template>
          <td v-else class="error">{{ reference.details }}</td>
        </template>
        <template v-if="searchData?.noteCount">
          <hr />
          <div
            v-for="(reference, referenceIndex) in searchResults.references"
            :key="referenceIndex"
            class="usj-container"
          >
            <NotesRenderer
              v-if="'content' in reference && reference.content"
              :contents="reference.content"
            />
          </div>
        </template>
      </template>
      <template v-else-if="searchResults.search_term">
        <h2>
          {{ searchResults.total_results }} results found for '{{ searchResults.search_term }}':
        </h2>
        <table>
          <tr v-for="(reference, referenceIndex) in searchResults.references" :key="referenceIndex">
            <td v-if="'invalid_reference' in reference" class="error" colspan="2">
              {{ reference.details }}
            </td>
            <template v-else>
              <td>
                <NuxtLink
                  :to="{
                    query: newQueryParamsForSearch(reference.reference),
                  }"
                  >{{ formatReference(reference.reference) }}</NuxtLink
                >
              </td>
              <td v-if="reference.content" class="usj-container">
                <UsjContentsRenderer
                  :contents="reference.content"
                  :text-direction="bibleTextDirection"
                  :highlights="reference.highlights"
                  :ignored-content-types="['note', 'chapter', 'verse']"
                />
              </td>
            </template>
          </tr>
        </table>
      </template>
    </div>
  </div>
</template>
