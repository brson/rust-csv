use std;
import std::io;
import std::io::{writer_util, reader_util};
import result;

export rowreader, rowaccess, rowiter, new_reader, new_reader_readlen;

enum state {
    fieldstart(bool),
    infield(uint, uint),
    inescapedfield(uint, uint),
    inquote(uint, uint)
}

type rowreader = {
    readlen: uint,
    delim: char,
    quote: char,
    f : io::reader,
    mutable offset : uint,
    mutable buffers : [@[char]],
    mutable state : state
};

type row = {
    fields : [ fieldtype ]
};

type bufferdescr = {
    escaped: bool,
    buffers: [@[char]],
    start: uint,
    end: uint
};

enum fieldtype {
    emptyfield(),
    bufferfield(bufferdescr)
}

iface rowiter {
    fn readrow() -> result::t<row, str>;
}

iface rowaccess {
    fn len() -> uint;
    fn getchars(uint) -> [char];
    fn getstr(uint) -> str;
}

fn new_reader(+f: io::reader, +delim: char, +quote: char) -> rowreader {
    {
        readlen: 1024u,
        delim: delim,
        quote: quote,
        f: f,
        mutable offset : 0u,
        mutable buffers : [],
        mutable state : fieldstart(false)
    }
}

fn new_reader_readlen(+f: io::reader, +delim: char, +quote: char, rl: uint) -> rowreader {
    {
        readlen: rl,
        delim: delim,
        quote: quote,
        f: f,
        mutable offset : 0u,
        mutable buffers : [],
        mutable state : fieldstart(false)
    }
}

impl of rowaccess for row {
    fn len() -> uint {
        vec::len(self.fields)
    }
    fn getchars(field: uint) -> [char] {
        fn unescape(escaped: [char]) -> [char] {
            let r : [char] = [];
            vec::reserve(r, vec::len(escaped));
            let in_q = false;
            for c in escaped { 
                if in_q { 
                    assert(c == '"');
                    in_q = false;
                } else {
                    in_q = c == '"';
                    r += [c];
                }
            }
            ret r;
        }
        alt self.fields[field] {
            emptyfield() { ret []; }
            bufferfield(desc) {
                let buf = [];
                { 
                    let i = 0u;
                    while i < vec::len(desc.buffers) {
                        let from = if (i == 0u)
                            { desc.start } else { 0u };
                        let to = if (i == vec::len(desc.buffers) - 1u)
                            { desc.end } else { vec::len(*desc.buffers[i]) };
                        buf += vec::slice(*desc.buffers[i], from, to);
                        i = i + 1u;
                    }
                }
                if desc.escaped {
                    buf = unescape(buf);
                }
                ret buf;
            }
        };
    }
    fn getstr(field: uint) -> str {
        ret str::from_chars(self.getchars(field));
    }
}

impl of rowiter for rowreader {
    fn readrow() -> result::t<row, str> {
        fn row_from_buf(self: rowreader, &fields: [fieldtype]) -> bool {
            fn new_bufferfield(self: rowreader, escaped: bool, sb: uint, so: uint, eo: uint) -> fieldtype {
                let eb = vec::len(self.buffers);
                let sb = sb, so = so, eo = eo;
                if escaped {
                    so += 1u;
                    if so > vec::len(*self.buffers[sb]) {
                        sb += 1u;
                        so = vec::len(*self.buffers[sb]) - 1u;
                    }
                    if eo > 0u {
                        eo -= 1u;
                    } else {
                        eb -= 1u;
                        eo = vec::len(*self.buffers[sb]) - 1u;
                    }
                }
                bufferfield({ escaped: escaped, buffers: vec::slice(self.buffers, sb, eb), start: so, end: eo })
            }
            let cbuffer = vec::len(self.buffers) - 1u;
            let buf: @[char] = self.buffers[cbuffer];
            while self.offset < vec::len(*buf) {
                let coffset = self.offset;
                let c : char = buf[coffset];
                self.offset += 1u;
                alt self.state {
                    fieldstart(after_delim) {
                        if c == self.quote {
                            self.state = inescapedfield(cbuffer, coffset);
                        } else if c == '\n' {
                            if after_delim {
                                fields += [emptyfield];
                            }
                            ret true;
                        } else if c == self.delim {
                            self.state = fieldstart(true);
                            fields += [emptyfield];
                        } else {
                            self.state = infield(cbuffer, coffset);
                        }
                    }
                    infield(b,o) {
                        if c == '\n' {
                            fields += [new_bufferfield(self, false, b, o, coffset)];
                            ret true;
                        } else if c == self.delim {
                            self.state = fieldstart(true);
                            fields += [new_bufferfield(self, false, b, o, coffset)];
                        }
                    }
                    inescapedfield(b, o) {
                        if c == self.quote {
                            self.state = inquote(b, o);
                        } else if c == self.delim {
                            self.state = fieldstart(true);
                            fields += [new_bufferfield(self, true, b, o, coffset)];
                        }
                    }
                    inquote(b, o) {
                        if c == '\n' {
                            fields += [new_bufferfield(self, true, b, o, coffset)];
                            ret true;
                        } else if c == self.quote {
                            self.state = inescapedfield(b, o);
                        } else if c == self.delim {
                            self.state = fieldstart(true);
                            fields += [new_bufferfield(self, true, b, o, coffset)];
                        }
                        // swallow odd chars, eg. space between field and "
                    }
                }
            }
            ret false;
        }

        self.state = fieldstart(false);
        let do_read = vec::len(self.buffers) == 0u;
        let fields = [];
        while true {
            if do_read {
                let data: @[char] = @self.f.read_chars(self.readlen);
                if vec::len(*data) == 0u {
                    ret result::err("EOF");
                }
                self.buffers += [data];
                self.offset = 0u;
            }

            if row_from_buf(self, fields) {
                let r: row = { fields: fields };
                fields = [];
                if vec::len(self.buffers) > 1u {
                    self.buffers = vec::slice(self.buffers, vec::len(self.buffers) - 1u, vec::len(self.buffers));
                }
                ret result::ok(r);
            }
            do_read = true;
        }
        ret result::err("unreachable");
    }
}

#[cfg(test)]
mod test {
    fn rowmatch(testdata: str, expected: [[str]]) {
        let chk = fn@(mk: fn(io::reader) -> rowreader) {
            let f = io::string_reader(testdata);
            let r = mk(f);
            let i = 0u;
            while true {
                let res = r.readrow();
                if result::failure(res) {
                    break;
                }
                let row = result::get(res);
                let expect = expected[i];

                assert(row.len() == vec::len(expect));
                let j = 0u;
                while j < row.len() {
                    assert(row.getstr(j) == expect[j]);
                    j += 1u;
                }
                i += 1u;
            }
            assert(i == vec::len(expected));
        };
        // test default reader params
        chk() { |inp|
            new_reader_readlen(inp, ',', '"', 2u)
        };
        // test continuations over read buffers
        let j = 1u;
        while j < str::len(testdata) {
            chk() { |inp|
                new_reader_readlen(inp, ',', '"', j)
            };
            j += 1u;
        }
    }

    #[test]
    fn test_simple() {
        rowmatch("a,b,c,d\n1,2,3,4\n",
                 [["a", "b", "c", "d"], ["1", "2", "3", "4"]]);
    }

    #[test]
    fn test_trailing_comma() {
        rowmatch("a,b,c,d\n1,2,3,4,\n",
                 [["a", "b", "c", "d"], ["1", "2", "3", "4", ""]]);
    }

    #[test]
    fn test_leading_comma() {
        rowmatch("a,b,c,d\n,1,2,3,4\n",
                 [["a", "b", "c", "d"], ["", "1", "2", "3", "4"]]);
    }

    #[test]
    fn test_quote_simple() {
        rowmatch("\"Hello\",\"There\"\na,b,\"c\",d\n",
                 [["Hello", "There"], ["a", "b", "c", "d"]]);
    }

    #[test]
    fn test_quote_nested() {
        rowmatch("\"Hello\",\"There is a \"\"fly\"\" in my soup\"\na,b,\"c\",d\n",
                 [["Hello", "There is a \"fly\" in my soup"], ["a", "b", "c", "d"]]);
    }

    #[test]
    fn test_blank_line() {
        rowmatch("\n\n", [[], []]);
    }
}





