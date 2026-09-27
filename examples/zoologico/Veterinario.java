import java.util.ArrayList;
import java.util.List;

public class Veterinario {
    private String nombre;
    private List<Animal> pacientes = new ArrayList<>();

    public Veterinario(String nombre) {
        this.nombre = nombre;
    }

    public void atender(Animal animal) {
        pacientes.add(animal);
        System.out.println("El veterinario " + nombre + " atiende a " + animal.getNombre());
    }

    public int cuantosPacientes() {
        return pacientes.size();
    }

    public List<Animal> getPacientes() {
        return pacientes;
    }
}