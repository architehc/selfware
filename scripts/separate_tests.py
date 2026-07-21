import os
import re

def process_file(filepath):
    with open(filepath, 'r', encoding='utf-8') as f:
        content = f.read()
    
    # Look for #[cfg(test)] followed by optional other attributes, then mod tests {
    pattern = re.compile(r'#\[cfg\(test\)\]\s*(?:#\[[^\]]*\]\s*)*mod\s+tests\s*\{')
    
    match = pattern.search(content)
    if not match:
        return False
        
    start_idx = match.start()
    brace_start = match.end() - 1 # Position of '{'
    
    # Find matching closing brace
    brace_count = 1
    end_idx = -1
    for i in range(brace_start + 1, len(content)):
        if content[i] == '{':
            brace_count += 1
        elif content[i] == '}':
            brace_count -= 1
            if brace_count == 0:
                end_idx = i
                break
                
    if end_idx == -1:
        print(f"Failed to find closing brace in {filepath}")
        return False
        
    # Extract test content
    test_content = content[brace_start+1:end_idx].strip() + "\n"
    
    # Generate test filename
    dir_name = os.path.dirname(filepath)
    base_name = os.path.basename(filepath)
    name_without_ext = os.path.splitext(base_name)[0]
    
    test_filename = f"{name_without_ext}_test.rs"
    test_filepath = os.path.join(dir_name, test_filename)
    
    # Write test file
    with open(test_filepath, 'w', encoding='utf-8') as f:
        f.write(test_content)
        
    # Replace in original file
    replacement = f'#[cfg(test)]\n#[path = "{test_filename}"]\nmod tests;\n'
    new_content = content[:start_idx] + replacement + content[end_idx+1:]
    
    with open(filepath, 'w', encoding='utf-8') as f:
        f.write(new_content)
        
    print(f"Refactored {filepath} -> {test_filename}")
    
    return True # Return true so we can loop in case of multiple inline test modules

def main():
    src_dir = 'src'
    count = 0
    for root, _, files in os.walk(src_dir):
        for file in files:
            if file.endswith('.rs') and not file.endswith('_test.rs'):
                filepath = os.path.join(root, file)
                while process_file(filepath):
                    count += 1
    print(f"Successfully separated {count} test modules.")

if __name__ == "__main__":
    main()
