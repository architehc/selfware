import os
import re

def process_file(filepath):
    with open(filepath, 'r') as f:
        content = f.read()

    # Find #[cfg(test)]\nmod tests { ... }
    # This regex is a bit simplistic, it assumes the file ends after the test block
    # or that we can balance braces. A brace matching algorithm is safer.
    
    match = re.search(r'#\[cfg\(test\)\]\s*mod tests\s*\{', content)
    if not match:
        return
        
    start_idx = match.end() - 1 # point to '{'
    
    # brace matching
    brace_count = 0
    end_idx = -1
    for i in range(start_idx, len(content)):
        if content[i] == '{':
            brace_count += 1
        elif content[i] == '}':
            brace_count -= 1
            if brace_count == 0:
                end_idx = i
                break
                
    if end_idx != -1:
        test_content = content[start_idx+1:end_idx].strip() + '\n'
        
        # generate path for tests
        dirname = os.path.dirname(filepath)
        basename = os.path.basename(filepath)
        name, ext = os.path.splitext(basename)
        test_filename = f"{name}_test{ext}"
        test_filepath = os.path.join(dirname, test_filename)
        
        # replace in original file
        new_content = content[:match.start()] + f'#[cfg(test)]\n#[path = "{test_filename}"]\nmod tests;\n' + content[end_idx+1:]
        
        with open(filepath, 'w') as f:
            f.write(new_content)
            
        with open(test_filepath, 'w') as f:
            f.write(test_content)
            
        print(f"Processed {filepath} -> {test_filepath}")

process_file("src/agent/context.rs")
