import type { books } from './books_data.js'

export type UsjRoot = {
  version: string
  content: UsjContent[]
}

export type UsjContent =
  | ({
      type: 'USJ'
    } & UsjRoot)
  | {
      type: 'para'
      marker:
        | 'ide'
        | 'sts'
        | 'rem'
        | 'h'
        | `toc${number}`
        | `toca${number}`
        | `imt${number}`
        | `is${number}`
        | 'ip'
        | 'ipi'
        | 'im'
        | 'imi'
        | 'ipq'
        | 'imq'
        | 'ipr'
        | 'ipc'
        | `iq${number}`
        | `ili${number}`
        | 'ib'
        | 'iot'
        | `io${number}`
        | 'iex'
        | 'imte'
        | 'ie'
        | `mt${number}`
        | `mte${number}`
        | 'cl'
        | 'cd'
        | `ms${number}`
        | 'mr'
        | `s${number}`
        | 'sr'
        | 'r'
        | 'd'
        | 'sp'
        | `sd${number}`
        | 'p'
        | 'm'
        | 'po'
        | 'cls'
        | 'pr'
        | 'pc'
        | 'pm'
        | 'pmo'
        | 'pmc'
        | 'pmr'
        | `pi${number}`
        | 'mi'
        | 'lit'
        | 'nb'
        | 'b'
        | `q${number}`
        | 'qr'
        | 'qc'
        | 'qa'
        | `qm${number}`
        | 'qd'
        | 'b'
        | 'lh'
        | `li${number}`
        | 'lf'
        | `lim${number}`
        | 'tr'
      content?: ParaContent[]
    }
  | ({
      type: 'char'
      content: [string] | []
    } & AttributesMap)
  | {
      type: 'book'
      marker: 'id'
      content: [string] | []
      code: Book
    }
  | {
      type: 'chapter'
      marker: 'c'
      number: `${number}`
      altnumber: number | null
      pubnumber: string | null
      sid: string
    }
  | {
      type: 'verse'
      marker: 'v'
      number: VerseRange
    }
  | ({
      type: 'ms'
      marker: `qt${number}` | 'ts'
      content?: ParaContent[]
    } & AttributesMap)
  | {
      type: 'note'
      marker: 'f' | 'fe' | 'ef' | 'x' | 'ex'
      caller: '+' | '-' | string
      category: string | null
    }
  | {
      type: 'table'
      content: UsjContent[]
    }
  | {
      type: 'table:row'
      marker:
        | `th${number}`
        | `thr${number}`
        | `thc${number}`
        | `tc${number}`
        | `tcr${number}`
        | `tcc${number}`
      content: UsjContent[]
    }
  | {
      type: 'table:cell'
      marker: 'string // TODO'
      content: ParaContent[]
      align: 'start' | 'center' | 'end'
    }
  | {
      type: 'sidebar'
      marker: 'esb'
      content: UsjContent[]
      category: string | null
    }
  | ({
      type: 'figure'
      marker: 'fig'
      content: [string] | []
    } & AttributesMap)
  | ({
      type: 'ref'
      content: [string] | []
    } & AttributesMap)

export type ParaContent = UsjContent | string

export type Book = {
  [K in keyof books]: books[K] extends { usfm_id: infer T } ? T : books[K]
}[keyof books]

export type VerseRange = `${number}-${number}`

export type AttributesMap = { [attribute: string]: string }
