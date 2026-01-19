<script lang="ts" setup>
import type { SearchResponse } from '~/bwfApi'

const route = useRoute()
const query = ref((route.query.q || '').toString())
const activeQuery = ref(query.value)
const {
  data: searchResults,
  pending,
  error,
} = await useFetch<SearchResponse>(
  () => `http://127.0.0.1:8080/v1/search?term=${activeQuery.value}`,
)

function search() {
  activeQuery.value = query.value

  const url = new URL(window.location.href)
  url.searchParams.set('q', query.value)
  history.pushState(null, '', url)
}
</script>

<template>
  <div>
    <h1>Search Page</h1>
    <input v-model="query" placeholder="Enter search term" @keyup.enter="search" />
    <button @click="search">Search</button>

    <div v-if="pending">Loading...</div>
    <div v-else-if="error">Error: {{ error.message }}</div>
    <div v-else>
      <h2>Search Results:</h2>
      <table v-if="searchResults!.response_type === 'search_results'">
        <tr v-for="(reference, referenceIndex) in searchResults!.references" :key="referenceIndex">
          <td v-if="'invalid_reference' in reference">{{ reference.details }}</td>
          <template v-else>
            <td>
              {{ reference.translated_book_name }} {{ reference.reference.chapter }}:{{
                reference.reference.verses
              }}
            </td>
            <td v-if="reference.content">
              <UsjContentsRenderer
                :contents="reference.content"
                :highlights="reference.highlights"
              />
            </td>
          </template>
        </tr>
      </table>
      <template v-else>
        <template
          v-for="(reference, referenceIndex) in searchResults!.references"
          :key="referenceIndex"
        >
          <hr v-if="referenceIndex > 0" />
          <template v-if="'content' in reference">
            <UsjContentsRenderer v-if="reference.content" :contents="reference.content" />
          </template>
        </template>
      </template>
    </div>
  </div>
</template>
